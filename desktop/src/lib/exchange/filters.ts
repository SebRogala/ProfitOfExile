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
 * Two ways of reading an empty input live here, and they are opposites. The
 * quality gates (POE-191, moved out of the server) are DEFAULT-ON: an unset
 * knob is that gate running at the value the server used to enforce for
 * everyone. The numeric bounds are DEFAULT-OFF: an unset bound is a filter the
 * reader is not running. `parseGate` and `parseAmount` each say why their side
 * is the way round it is — check which one you are in before reusing either.
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
 * wants, so the value is free to spell "no ceiling".
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
 * The gate values the server used to enforce for everyone.
 *
 * Data rather than five literals spread through the parsers, because three
 * callers need the same five numbers and they must not be able to disagree:
 * the parser's fallback, the filter bar's "Defaults" reset, and the badge that
 * counts how many knobs the reader has moved off default.
 *
 * `minRoiPct` is 2 for the same reason the other four carry the server's old
 * numbers — the server gated 2% regardless of what the reader had typed, so a
 * reader who never set it was already looking at a 2% table.
 *
 * These five numbers are pinned in Go too:
 * TestBestPlays_recordedHourUnderTheClientsDefaultLevels_yieldsNoOneHopRoutes
 * arms the same levels to prove untouched knobs give the old server's answer
 * (ADR-015). Changing a default here is changing that test.
 */
export const gateDefaults: Gates = {
	minRoiChaos: 3,
	minTurnover: 10000,
	maxTickPct: 10,
	minEdgeTickRatio: 5,
	minRoiPct: 2
};

/**
 * The gate knobs as they are stored — the raw `persisted()` strings (ADR-013),
 * one per field of `Gates`.
 */
export type GateInputs = { [K in keyof Gates]: string };

/**
 * One gate knob as a number.
 *
 * DEFAULT-ON, which is the OPPOSITE of `parseAmount` below: blank and
 * unparseable both answer the knob's DEFAULT here, and "off" there. The two
 * contracts live in one file on purpose, so read which one you are in before
 * copying either.
 *
 * A gate is standing policy. The reader who has never opened the Gates row is
 * entitled to the table the server used to hand them, and a knob that read an
 * unset preference as "off" would drop that policy on every fresh install and
 * bury the table under fragment noise — the defaults are the trash-killer.
 * A numeric bound is a moment filter: the reader typed it to answer one
 * question, so an empty box means they are done asking.
 *
 * `0` is therefore the explicit off, and has to survive parsing: it is the only
 * way for the reader to say "show me the cheap fragments too".
 *
 * A negative knob is read as off rather than kept. For the four floors the
 * server's positivity floor means no served play carries a negative return, so
 * a floor below zero could never drop a row — "off" is the honest name for a
 * filter that cannot fire. For the one ceiling (maxTickPct) a negative would
 * fire against EVERY play and empty the table; a stored negative there is
 * garbage, not a request, and off is the recovery.
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
 * because the two halves mean opposite things about an empty input: a gate
 * unset is a gate ON at its default, a bound unset is a bound OFF. Folding them
 * together would put both contracts inside one filter pass where the next
 * reader has to remember which field obeys which — and the page reflects the
 * same split, since Clear empties the bounds while Defaults restores the gates.
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
 * Every comparison is inclusive on the passing side: a play sitting exactly on
 * a floor met it, and the server it inherits these numbers from cleared the
 * same plays.
 *
 * `maxTickPct` and `minRoiPct` cross scales here and nowhere else — the inputs
 * are percent points, the wire's `tick` and `roiPct` are fractions.
 * `minEdgeTickRatio` does not cross anything: a bare ratio times a fraction is
 * already a fraction.
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
 * The return floor is NOT one of them: it is `Gates.minRoiPct`, on the
 * default-on side of the file, and lives there alone so the reader's 2% floor
 * has exactly one owner.
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
	/**
	 * @deprecated POE-192 removed the Quantity stepper; the bounds read
	 * `worthwhileScale` instead. Ignored here, and accepted only so the page's
	 * still-passing object literal compiles until chunk 3 stops passing it.
	 */
	quantity?: number;
	/**
	 * @deprecated POE-192 removed the Min gain input: the per-flip floor is
	 * `Gates.minRoiChaos` and the run-level floor is now the fixed scale target,
	 * so a third gain knob had nothing left to say. Ignored here; chunk 3 removes
	 * the input, the `currencyExchangeMinGain` preference and this field.
	 */
	minGain?: string;
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
 * A play whose scale cannot be derived — `roi ≤ 0`, which the server's
 * positivity floor (ADR-015) never serves — falls back to its per-exchange
 * investment rather than being dropped or waved through: that is what such a
 * play demonstrably ties up, and reading it that way keeps the bound a
 * comparison against a real figure instead of a second hidden gate.
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

/**
 * Whether the reader's quantity is more than the play's thinnest leg cleared in
 * the hour the prices come from.
 *
 * Equal is not over: a depth of 40 means 40 units changed hands, so 40 is a
 * quantity the book demonstrably supported. The row is marked, never dropped —
 * depth is last hour's evidence, not this hour's limit.
 *
 * @deprecated POE-192: absorption speaks through the Scale column's hours now,
 * so the amber Depth cell and its footnote go. Kept only so the page compiles
 * until chunk 3 removes that marking, which deletes this with it.
 */
export function overDepth(play: CurrencyExchangePlay, quantity: number): boolean {
	return quantity > play.depth;
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
