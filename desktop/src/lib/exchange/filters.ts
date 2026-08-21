/**
 * The Currency Exchange page's client-side filtering (POE-186).
 *
 * The server ranks; this file only narrows what the reader looks at. Every
 * predicate reads a wire field or the one derived figure the page is built
 * around — `worthwhileScale` from `./view`, this file's single runtime import
 * of it — and nothing here is fetched: the filters run over the list already
 * on screen, so a rule that empties the table is a rule the reader can undo
 * without a round trip.
 *
 * Two layers of rules, one verdict. A category rule paints all sixteen sidebar
 * groups; an item rule names one exchange id and beats the category it belongs
 * to. Both live in `persisted()` strings (ADR-013), which is why the parsers
 * take raw text and answer with an empty rule set rather than throwing.
 *
 * Two ways of reading an empty input live here, and since POE-193 they agree on
 * the answer without agreeing on the reasoning. The quality gates (POE-191,
 * moved out of the server) ship OFF: an unset knob is a gate the reader has not
 * armed, and `gateDefaults` is the one place a build says what unset runs at.
 * The numeric bounds are off by construction: they have no default to consult,
 * because an unset bound is a filter the reader is not running. `parseGate` and
 * `parseAmount` each say what their side does with input that is NOT blank —
 * that is where the two still differ, so check which one you are in before
 * reusing either.
 *
 * Pure TypeScript on purpose — no Svelte runes, no Tauri imports — so the whole
 * file is reachable from vitest without a component harness, the same reason
 * `view.ts` gives.
 */
import type { CurrencyExchangePlay } from '$lib/api';
import { worthwhileScale } from './view';
import type { ExchangeUnit } from './view';

// -------------------------------------------------------------- the rules --

/**
 * What a rule says about the ids it covers. Absence is the third state
 * (neutral) and is spelled as a missing key, never as a stored `'neutral'` — a
 * neutral rule and no rule are the same fact, and storing both would let them
 * disagree.
 *
 * The item layer reuses this union: the two layers differ in what they key on,
 * not in what they can say.
 */
export type CategoryRuleState = 'only' | 'hide';

/** Category name (as the response's `categories` spells it) → its rule. */
export type CategoryRules = Record<string, CategoryRuleState>;

/**
 * One item-level rule.
 *
 * `name` is stored beside the id because the chip has to render when the item
 * is absent from the current response — a rule survives a horizon change, a
 * mode change and a restart, and an id alone would draw a chip labelled
 * `Metadata/Items/...`. The icon is deliberately not stored: it is only ever
 * available for items the current response carries, and a stale path would
 * request artwork the server may no longer cache.
 */
export interface ItemRule {
	id: string;
	name: string;
	state: CategoryRuleState;
}

function isRuleState(value: unknown): value is CategoryRuleState {
	return value === 'only' || value === 'hide';
}

/**
 * Read the persisted category rules.
 *
 * Anything that is not an object of known states — an empty preference, a
 * half-written string, a shape from an older build, a state this build has
 * since dropped — degrades to "no rules" rather than throwing on the page's
 * first render. Unknown *categories* are kept: the taxonomy is the server's, so
 * a category this build has never seen is a rule about a category that exists.
 * An empty-string key is dropped, because `""` is the wire's "uncategorised"
 * marker and never a category to rule on.
 */
export function parseCategoryRules(raw: string): CategoryRules {
	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return {};
	}
	if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};

	const rules: CategoryRules = {};
	for (const [category, state] of Object.entries(parsed as Record<string, unknown>)) {
		if (category !== '' && isRuleState(state)) rules[category] = state;
	}
	return rules;
}

/** The storable form of `parseCategoryRules`. */
export function serializeCategoryRules(rules: CategoryRules): string {
	return JSON.stringify(rules);
}

/**
 * Read the persisted item rules.
 *
 * Entry-by-entry rather than all-or-nothing: one malformed chip written by an
 * older build should cost the reader that chip, not every rule they have set.
 * A rule with no id can match nothing, so it is dropped too.
 */
export function parseItemRules(raw: string): ItemRule[] {
	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return [];
	}
	if (!Array.isArray(parsed)) return [];

	const rules: ItemRule[] = [];
	for (const entry of parsed) {
		if (typeof entry !== 'object' || entry === null) continue;
		const { id, name, state } = entry as Record<string, unknown>;
		if (typeof id !== 'string' || id === '') continue;
		if (!isRuleState(state)) continue;
		rules.push({ id, name: typeof name === 'string' ? name : id, state });
	}
	return rules;
}

/** The storable form of `parseItemRules`. */
export function serializeItemRules(rules: ItemRule[]): string {
	return JSON.stringify(rules);
}

/**
 * The next state of a pill the reader clicked: neutral → only → hide →
 * neutral.
 *
 * `undefined` is neutral on both ends, so a caller applies the answer by
 * deleting the key when it comes back undefined. Only reaching hide through
 * only costs one extra click on the way to hiding a group, and buys a cycle
 * that returns to where it started — a two-state pill would need a separate
 * control to clear itself.
 */
export function cycleCategoryRule(
	current: CategoryRuleState | undefined
): CategoryRuleState | undefined {
	if (current === undefined) return 'only';
	return current === 'only' ? 'hide' : undefined;
}

// -------------------------------------------------------------- the sides --

/** Which half of a leg a side is. A rule matches in either role. */
export type SideRole = 'item' | 'quote';

/** One side of one leg: what the filters match on, and what the picker lists. */
export interface PlaySide {
	id: string;
	name: string;
	icon: string | null;
	category: string;
	role: SideRole;
}

/**
 * Every distinct side of a play — both halves of every leg.
 *
 * Both halves, because the reader shops for whichever side they hold: a play
 * that buys Chaos Orbs with Divine Orbs is a Divine play to someone spending
 * divine and a Chaos play to someone stocking chaos, and a rule naming either
 * one has to catch it.
 *
 * Distinct by id *and* role: a currency that appears as the item of one leg and
 * the quote of another is two facts about the play, and the picker shows the
 * role it was found in. Rule matching does not care — it reads ids — so the
 * duplicate id costs nothing there.
 */
export function playSides(play: CurrencyExchangePlay): PlaySide[] {
	const sides: PlaySide[] = [];
	const seen = new Set<string>();

	const add = (side: PlaySide) => {
		const key = `${side.role}:${side.id}`;
		if (seen.has(key)) return;
		seen.add(key);
		sides.push(side);
	};

	for (const leg of play.legs) {
		add({
			id: leg.item,
			name: leg.itemName,
			icon: leg.itemIcon,
			category: leg.itemCategory,
			role: 'item'
		});
		add({
			id: leg.quote,
			name: leg.quoteName,
			icon: leg.quoteIcon,
			category: leg.quoteCategory,
			role: 'quote'
		});
	}
	return sides;
}

/**
 * What the two layers say about one side, resolved.
 *
 * The item layer beats the category layer — that is what makes "hide Currency,
 * keep Imperial Legacy" expressible at all — and a side whose category is `""`
 * (an id the server's item asset does not cover) inherits nothing: it is not
 * filed under a seventeenth group, so no category rule reaches it.
 *
 * The caller passes the item rule it has already looked up rather than the
 * whole list, so a filter pass over hundreds of plays keeps its one map and the
 * picker keeps its one `find`. Both then read precedence from here instead of
 * spelling it out twice — the picker marks a row hidden exactly when this
 * answers `'hide'`.
 */
export function effectiveRule(
	itemRule: CategoryRuleState | undefined,
	category: string,
	categoryRules: CategoryRules
): CategoryRuleState | undefined {
	if (itemRule !== undefined) return itemRule;
	return category === '' ? undefined : categoryRules[category];
}

/**
 * Whether an item rule is doing work the category layer would not have done —
 * what the filter bar's chip badges itself with.
 *
 * True exactly when the item's category carries an EXPLICIT rule that says
 * something else: an Only inside a hidden category, and a Hide inside a
 * category ruled Only. A neutral category is not a disagreement — an Only chip
 * in an unruled category is the only thing saying anything about that item, so
 * badging it would call every chip an override.
 *
 * `category` is `undefined` for an item the current response does not carry: a
 * rule outlives the response that created it, and a chip whose category is
 * unknown cannot be shown to contradict one. Uncategorised (`''`) is false for
 * the reason `effectiveRule` gives — no category rule reaches it.
 */
export function overridesCategory(
	rule: ItemRule,
	category: string | undefined,
	categoryRules: CategoryRules
): boolean {
	if (category === undefined || category === '') return false;
	const categoryRule = categoryRules[category];
	return categoryRule !== undefined && categoryRule !== rule.state;
}

/**
 * Apply the two rule layers.
 *
 * The pinned semantics, in the order they resolve:
 * 1. Every side of the play gets a verdict, per `effectiveRule`.
 * 2. One `hide` verdict anywhere hides the play. Hide beats Only: a play the
 *    reader has said they will not trade is not rescued by another leg they
 *    would.
 * 3. If any `only` rule exists in *either* layer, a play must carry at least
 *    one `only` verdict to survive. Only is a whitelist over the whole table,
 *    not a per-layer one — otherwise setting an item to Only would leave every
 *    unrelated play on screen and read as a no-op.
 */
export function applyRules(
	plays: CurrencyExchangePlay[],
	categoryRules: CategoryRules,
	itemRules: ItemRule[]
): CurrencyExchangePlay[] {
	const byItem = new Map<string, CategoryRuleState>();
	for (const rule of itemRules) byItem.set(rule.id, rule.state);

	const hasOnly =
		itemRules.some((rule) => rule.state === 'only') ||
		Object.entries(categoryRules).some(([category, state]) => category !== '' && state === 'only');

	return plays.filter((play) => {
		let matchedOnly = false;
		for (const side of playSides(play)) {
			const verdict = effectiveRule(byItem.get(side.id), side.category, categoryRules);
			if (verdict === 'hide') return false;
			if (verdict === 'only') matchedOnly = true;
		}
		return hasOnly ? matchedOnly : true;
	});
}

// -------------------------------------------------------------- the gates --

/** Percent points per unit fraction — the scale the percent inputs are typed in. */
const PERCENT_PER_FRACTION = 100;

/**
 * The five quality gates, parsed.
 *
 * These are the floors the SERVER used to apply before it served anything
 * (POE-191 moved them here: the server now serves everything sane — liveness,
 * persistence, positivity, cap — and the reader owns the quality bar). Nothing
 * here re-derives a market number either; every gate reads a field the wire
 * already carries per play, which is what made the move a comparison rather
 * than a recalculation.
 *
 * `0` is off for every knob, `maxTickPct` included: a spread ceiling of zero
 * would ask for a market with no spread at all, which is not a table anyone
 * wants, so the value is free to spell "no ceiling". `applyGates` spells that
 * out as a `> 0` guard per comparison, which is what lets all five ship at 0.
 */
export interface Gates {
	/** Chaos gained per exchange, floor. */
	minRoiChaos: number;
	/** Chaos that changed hands in the hour, floor. */
	minTurnover: number;
	/** Spread ceiling, typed as a PERCENT (10 = 10%). */
	maxTickPct: number;
	/** How many times the spread the return must be, floor. */
	minEdgeTickRatio: number;
	/** Return floor, typed as a PERCENT (2 = 2%). */
	minRoiPct: number;
}

/**
 * What an unset knob runs at: nothing. All five gates ship OFF (POE-193).
 *
 * Data rather than five literals spread through the parsers, because three
 * callers need the same five numbers and they must not be able to disagree:
 * the parser's fallback, the filter bar's "Defaults" reset, and the badge that
 * counts how many knobs the reader has moved off default.
 *
 * Until POE-193 these carried the levels the server used to enforce for
 * everyone (3 / 10,000 / 10% / 5× / 2%) and shipped them armed, so the
 * out-of-the-box table was the old server's table. The rule now is that
 * everything the server serves is VISIBLE out of the box, because the judgement
 * those levels stood in for is made honestly one layer down: the ranking is the
 * per-play simulated expectation, and lowCoverage, suspect and a negative
 * expectation are flagged and sunk rather than hidden (ADR-016). An absolute
 * cutoff in front of a measured ranking hides plays the measurement says are
 * real — measured against the live stack on 2026-08-21, the armed levels hid
 * 142 of the 143 served 1-hop plays, and hid an Apocalypse card flip for
 * turning over 8,532 chaos in the hour against a 10,000 line.
 *
 * The levels are not gone, they are RECOMMENDED rather than shipped, and the
 * meaning is what a reader arms rather than the number:
 * - `minRoiChaos` 3 — a payout under a few chaos is a rounding error.
 * - `minTurnover` 10,000 — under it you are not joining a market, you are it.
 * - `maxTickPct` 10 — a coarser price step cannot be undercut finely.
 * - `minEdgeTickRatio` 5 — a return narrower than five steps is quantization.
 * - `minRoiPct` 2 — the return floor the server applied to everyone.
 * They come from POE-184's 30,534 priced market-hours (ADR-015); typing all
 * five reproduces the pre-POE-193 table exactly.
 *
 * Go arms the same five on a recorded hour —
 * TestBestPlays_recordedHourUnderTheOldServerLevels_yieldsNoOneHopRoutes — but
 * that is no longer the same claim as this constant. It documents what the
 * levels CUT when a reader arms them; it does not pin what this file ships.
 */
export const gateDefaults: Gates = {
	minRoiChaos: 0,
	minTurnover: 0,
	maxTickPct: 0,
	minEdgeTickRatio: 0,
	minRoiPct: 0
};

/**
 * The gate knobs as they are stored — the raw `persisted()` strings (ADR-013),
 * one per field of `Gates`.
 */
export type GateInputs = { [K in keyof Gates]: string };

/**
 * One gate knob as a number.
 *
 * Blank and unparseable both answer the knob's DEFAULT, which since POE-193 is
 * 0 — off — for all five. That reaches the same place `parseAmount` reaches by
 * a different route, and the route is the part worth keeping: this side asks a
 * BUILD what an unset knob runs at, the other side has no such value to ask
 * for. Keep the fallback rather than hard-coding 0 here. The knobs persist as
 * `''` precisely so a build that changes what unset means reaches the reader
 * who never touched one, instead of leaving them on a number an older build
 * wrote into their settings file.
 *
 * `0` still has to survive parsing rather than reading as unset: a reader who
 * typed it is saying "show me the cheap fragments too", and the box they typed
 * it into has to keep saying so.
 *
 * A negative knob is read as off rather than kept. For the four floors the
 * server's positivity floor means no served play carries a negative `roiPct` or
 * `roi` — the two numbers these gates judge — so a floor below zero could never
 * drop a row; "off" is the honest name for a filter that cannot fire. For the
 * one ceiling (maxTickPct) a negative would fire against EVERY play and empty
 * the table; a stored negative there is garbage, not a request, and off is the
 * recovery.
 *
 * Read that as a statement about the GATE VALUES, not about the plays. Since
 * POE-193 a served play CAN carry a negative `expectedRoi` — the fill-simulated
 * expectation is free to measure a loss, and ADR-016 serves and flags it rather
 * than hiding it. No gate here reads that field, so the reasoning above is
 * untouched: the negative that cannot occur is the one on the optimistic pair
 * these five knobs compare against.
 */
export function parseGate(raw: string, fallback: number): number {
	if (raw.trim() === '') return fallback;
	const value = Number(raw);
	if (!Number.isFinite(value)) return fallback;
	return value < 0 ? 0 : value;
}

/** Every gate knob, read against its own default. */
export function parseGates(inputs: GateInputs): Gates {
	return {
		minRoiChaos: parseGate(inputs.minRoiChaos, gateDefaults.minRoiChaos),
		minTurnover: parseGate(inputs.minTurnover, gateDefaults.minTurnover),
		maxTickPct: parseGate(inputs.maxTickPct, gateDefaults.maxTickPct),
		minEdgeTickRatio: parseGate(inputs.minEdgeTickRatio, gateDefaults.minEdgeTickRatio),
		minRoiPct: parseGate(inputs.minRoiPct, gateDefaults.minRoiPct)
	};
}

/**
 * Apply the quality gates.
 *
 * Its own function rather than five more fields on `applyNumericFilters`,
 * because the two halves are set and unset by different controls: the gates are
 * a standing quality bar with their own Defaults reset and their own explicit
 * off (`0`), the bounds are a question the reader asked once and Clear sweeps
 * up. Folding them together would put both contracts inside one filter pass
 * where the next reader has to remember which field obeys which — and the page
 * reflects the same split, since Clear empties the bounds and leaves the gates
 * where the reader put them.
 *
 * A play passes only by clearing every gate that is on:
 * - `roi ≥ minRoiChaos` — the chaos ONE exchange gains, never the scaled figure:
 *   a gate is about whether the market is worth trading at all, and the size it
 *   has to be repeated to be worth doing is the Scale column's answer, not this
 *   one's.
 * - `turnover ≥ minTurnover` — chaos that changed hands in the hour.
 * - `tick ≤ maxTickPct / 100` — the spread ceiling.
 * - `roiPct ≥ minEdgeTickRatio × tick` — the return has to be a multiple of the
 *   spread it has to cross, which is what keeps a play whose whole edge is one
 *   price step off the table.
 * - `roiPct ≥ minRoiPct / 100` — the return floor.
 *
 * A gate at 0 does not run at all, which is what makes 0 the shipped default
 * (`gateDefaults`) as well as the reader's explicit off — the `> 0` guard is
 * the off switch, one per comparison, `maxTickPct` included.
 *
 * Every comparison is inclusive on the passing side: a play sitting exactly on
 * a floor met it, and the server these numbers were taken from cleared the same
 * plays when it enforced them.
 *
 * `maxTickPct` and `minRoiPct` cross scales here and nowhere else — the inputs
 * are percent points, the wire's `tick` and `roiPct` are fractions.
 * `minEdgeTickRatio` does not cross anything: a bare ratio times a fraction is
 * already a fraction.
 *
 * Every gate still judges the OPTIMISTIC `roiPct`/`roi`, deliberately, even
 * though the server now ranks on `expectedRoi` (POE-193). The levels a reader
 * arms are the server's old ones and were calibrated against those two fields;
 * the Go test that arms them measures the same numbers, so re-pointing a gate
 * at the expectation would silently change what a typed level cuts. What the
 * expectation is for is the ORDER and the Exp. ROI column — the reader sees the
 * measured outcome and decides, which is ADR-015's split with the bar still on
 * the reader's side, and since POE-193 with nothing on that bar until they put
 * it there.
 */
export function applyGates(plays: CurrencyExchangePlay[], gates: Gates): CurrencyExchangePlay[] {
	return plays.filter((play) => {
		if (gates.minRoiChaos > 0 && play.roi < gates.minRoiChaos) return false;
		if (gates.minTurnover > 0 && play.turnover < gates.minTurnover) return false;
		if (gates.maxTickPct > 0 && play.tick > gates.maxTickPct / PERCENT_PER_FRACTION) return false;
		if (gates.minEdgeTickRatio > 0 && play.roiPct < gates.minEdgeTickRatio * play.tick) return false;
		if (gates.minRoiPct > 0 && play.roiPct < gates.minRoiPct / PERCENT_PER_FRACTION) return false;
		return true;
	});
}

// ------------------------------------------------------------ the numbers --

/**
 * The numeric filter bar's inputs.
 *
 * Both bounds arrive as the raw persisted strings, not as numbers: they are
 * `persisted()` values bound to text inputs, and "" (never set, or cleared)
 * has to mean "filter off" rather than 0. Parsing them here keeps that one
 * boundary in one place instead of spreading `=== ''` checks through the page.
 *
 * The return floor is NOT one of them: it is `Gates.minRoiPct`, on the gate
 * side of the file, and lives there alone so the reader's return floor has
 * exactly one owner.
 */
export interface NumericFilters {
	/**
	 * Investment bounds, typed in `unit`, compared against the play's WORTHWHILE
	 * SCALE — see `applyNumericFilters`.
	 */
	investMin: string;
	investMax: string;
	unit: ExchangeUnit;
	/** The response's newest-hour chaos value of one divine; 0 when unknown. */
	divineChaosRate: number;
}

/**
 * A filter input as a number, or `null` for "not set".
 *
 * Blank and unparseable both answer `null` — a half-typed "1e" must not filter
 * the table down to nothing while the reader is still typing the exponent.
 * Negative bounds are taken at face value; ROI can be negative, and a min of
 * -5% is a real thing to ask for.
 */
function parseAmount(raw: string): number | null {
	if (raw.trim() === '') return null;
	const value = Number(raw);
	return Number.isFinite(value) ? value : null;
}

/**
 * Apply the investment bounds.
 *
 * The bounds are compared against the play's WORTHWHILE SCALE (POE-192), not
 * against one exchange: the question a bankroll asks is "can I afford to run
 * this play until it pays", and since the app derives that size rather than
 * asking for it, the size the bound has to meet is the derived one. A 2c
 * fragment flip whose per-exchange cost is 40c ties up 4,000c by the time it has
 * cleared the target, and a 500c ceiling that let it through would be answering
 * about a trip the reader would never make.
 *
 * A play whose scale cannot be derived falls back to its per-exchange
 * investment rather than being dropped or waved through. Since POE-193 that
 * condition is `expectedRoi ≤ 0`, and it is a case the table now HITS: the
 * server's positivity floor governs the optimistic `roi`, while the simulated
 * expectation is free to measure a loss and the play is served anyway — 7.9% of
 * the calibration set realized negative (ADR-016). Falling back is therefore a
 * deliberate choice about a live case and not a guard against an impossible
 * one: dropping such a play here would be a sixth quality gate, hidden inside a
 * bankroll bound the reader set to answer a different question, and hiding the
 * measured losers is exactly what serve-and-flag exists to prevent.
 *
 * The asymmetry that buys is worth naming: a losing play is then judged by what
 * ONE exchange costs, while every other row is judged by what its whole run
 * ties up. A ceiling can consequently keep a negative-expectation play it would
 * have dropped had the play been worth repeating. That is the honest reading —
 * there is no run size for a play with nothing to run toward — and the Exp. ROI
 * column is where the reader sees why the row is there.
 *
 * The `unit` toggle converts the bounds, never the play: the wire's
 * `investment` is always chaos. A rate of 0 (the newest hour carried no
 * divine/chaos trade) makes divine unconvertible, so the bounds are read as
 * chaos instead of being multiplied by nothing and silently passing every play.
 *
 * POE-192 changed what the bounds MEASURE — the worthwhile run's investment,
 * not one exchange's — which is why the page stores them under new pref keys:
 * a ceiling typed against the old meaning must not silently empty the new
 * table.
 */
export function applyNumericFilters(
	plays: CurrencyExchangePlay[],
	filters: NumericFilters
): CurrencyExchangePlay[] {
	const { unit, divineChaosRate } = filters;
	const rate = unit === 'divine' && divineChaosRate > 0 ? divineChaosRate : 1;

	const investMin = parseAmount(filters.investMin);
	const investMax = parseAmount(filters.investMax);
	if (investMin === null && investMax === null) return [...plays];

	return plays.filter((play) => {
		const investment = worthwhileScale(play)?.investment ?? play.investment;
		if (investMin !== null && investment < investMin * rate) return false;
		if (investMax !== null && investment > investMax * rate) return false;
		return true;
	});
}

// ------------------------------------------------------------- the search --

/**
 * Whether a play mentions what the reader typed.
 *
 * A VIEW filter, not a rule: it composes after the two rule layers and the
 * numeric bounds, nothing about it is persisted, and Clear leaves it alone — a
 * search is a moment, not a setup.
 *
 * Case-insensitive substring over the DISPLAY names of both halves of every
 * leg, both halves for the reason `playSides` gives: the reader shops for
 * whichever side they hold, and a 1-hop play is worth finding by the currency
 * it passes through as well as by its ends. Names only, never the
 * `Metadata/Items/...` ids — the reader types what the row shows them, so an id
 * hit would be a row with nothing on screen to explain why it survived.
 *
 * An empty or whitespace-only query matches every play, so a box the reader has
 * cleared (or left a space in) is a filter that is off rather than one that
 * empties the table. The query is normalised per play rather than by the
 * caller, which keeps the signature a plain predicate: callers pass the raw
 * input string and have nothing to remember.
 */
export function matchesSearch(play: CurrencyExchangePlay, query: string): boolean {
	const needle = query.trim().toLowerCase();
	if (needle === '') return true;
	return playSides(play).some((side) => side.name.toLowerCase().includes(needle));
}

// ------------------------------------------------------------ the universe --

/** One row of the item picker. */
export interface ExchangeItem {
	id: string;
	name: string;
	icon: string | null;
	category: string;
	/** How many of the current response's plays this item appears in. */
	playCount: number;
}

/**
 * The distinct items the current response's plays are made of, by name.
 *
 * The picker searches this, not the server's full asset: the client is never
 * sent the whole item list, and an item absent from every play cannot change
 * which plays are visible — a rule on it would be a chip that does nothing.
 *
 * Counted per play, not per side: an item that is both halves of a round trip
 * is still one play's worth of reason to keep it. Icon and category come from
 * the first side that carries one, so a leg the asset could not decorate does
 * not blank a row another leg filled in; the name is the first side's as-is.
 */
export function itemUniverse(plays: CurrencyExchangePlay[]): ExchangeItem[] {
	const byId = new Map<string, ExchangeItem>();

	for (const play of plays) {
		const counted = new Set<string>();
		for (const side of playSides(play)) {
			const existing = byId.get(side.id);
			if (existing === undefined) {
				byId.set(side.id, {
					id: side.id,
					name: side.name,
					icon: side.icon,
					category: side.category,
					playCount: 0
				});
			} else {
				if (existing.icon === null) existing.icon = side.icon;
				if (existing.category === '') existing.category = side.category;
			}
			if (!counted.has(side.id)) {
				counted.add(side.id);
				// Non-null: the entry was just created above if it was missing.
				byId.get(side.id)!.playCount += 1;
			}
		}
	}

	// By name because that is what the reader types; by id on a tie so two
	// items sharing a display name keep a stable order between renders.
	return [...byId.values()].sort((a, b) => a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}
