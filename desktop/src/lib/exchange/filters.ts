/**
 * The Currency Exchange page's client-side filtering (POE-186).
 *
 * The server ranks; this file only narrows what the reader looks at. Nothing
 * here re-derives a market number — every predicate reads a field the wire
 * already carries — and nothing here is fetched: the filters run over the list
 * already on screen, so a rule that empties the table is a rule the reader can
 * undo without a round trip.
 *
 * Two layers of rules, one verdict. A category rule paints all sixteen sidebar
 * groups; an item rule names one exchange id and beats the category it belongs
 * to. Both live in `persisted()` strings (ADR-013), which is why the parsers
 * take raw text and answer with an empty rule set rather than throwing.
 *
 * Pure TypeScript on purpose — no Svelte runes, no Tauri imports — so the whole
 * file is reachable from vitest without a component harness, the same reason
 * `view.ts` gives.
 */
import type { CurrencyExchangePlay } from '$lib/api';
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
 * Apply the two rule layers.
 *
 * The pinned semantics, in the order they resolve:
 * 1. Every side of the play gets a verdict — its item rule if it has one, else
 *    its category's rule. The item layer beats the category layer, which is
 *    what makes "hide Currency, keep Imperial Legacy" expressible at all.
 * 2. One `hide` verdict anywhere hides the play. Hide beats Only: a play the
 *    reader has said they will not trade is not rescued by another leg they
 *    would.
 * 3. If any `only` rule exists in *either* layer, a play must carry at least
 *    one `only` verdict to survive. Only is a whitelist over the whole table,
 *    not a per-layer one — otherwise setting an item to Only would leave every
 *    unrelated play on screen and read as a no-op.
 *
 * A side whose category is `""` (an id the server's asset does not cover)
 * simply has no category rule to inherit; it is not silently filed under a
 * sixteenth group, and it is never hidden by a category rule it is not in.
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
			const verdict =
				byItem.get(side.id) ?? (side.category === '' ? undefined : categoryRules[side.category]);
			if (verdict === 'hide') return false;
			if (verdict === 'only') matchedOnly = true;
		}
		return hasOnly ? matchedOnly : true;
	});
}

// ------------------------------------------------------------ the numbers --

/**
 * The numeric filter bar's inputs.
 *
 * The four bounds arrive as the raw persisted strings, not as numbers: they are
 * `persisted()` values bound to text inputs, and "" (never set, or cleared) has
 * to mean "filter off" rather than 0. Parsing them here keeps that one boundary
 * in one place instead of spreading `=== ''` checks through the page.
 */
export interface NumericFilters {
	/** Exchanges the reader intends to run; multiplies investment and ROI. */
	quantity: number;
	/** Investment bounds, typed in `unit`, compared AT quantity. */
	investMin: string;
	investMax: string;
	unit: ExchangeUnit;
	/** The response's newest-hour chaos value of one divine; 0 when unknown. */
	divineChaosRate: number;
	/** Minimum return, typed as a PERCENT (5 = 5%). */
	minRoiPct: string;
	/** Minimum chaos gained across the whole quantity. */
	minGain: string;
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

/** Percent points per unit fraction — the scale the ROI% input is typed in. */
const PERCENT_PER_FRACTION = 100;

/**
 * Apply the investment, ROI% and gain bounds.
 *
 * Investment and gain are compared AT QUANTITY: the reader sets the quantity to
 * say how many exchanges they intend to run, so "at most 500c" is a question
 * about what the run costs, not about one unit of it.
 *
 * The `unit` toggle converts the investment bounds only — `minGain` is chaos by
 * definition, and the wire's `investment`/`roi` are always chaos. A rate of 0
 * (the newest hour carried no divine/chaos trade) makes divine unconvertible,
 * so the bounds are read as chaos instead of being multiplied by nothing and
 * silently passing every play.
 *
 * `minRoiPct` crosses scales here and nowhere else: the input is percent
 * points, `play.roiPct` is a fraction, and the division lives at this boundary
 * so no caller has to remember which side it is on.
 */
export function applyNumericFilters(
	plays: CurrencyExchangePlay[],
	filters: NumericFilters
): CurrencyExchangePlay[] {
	const { quantity, unit, divineChaosRate } = filters;
	const scale = unit === 'divine' && divineChaosRate > 0 ? divineChaosRate : 1;

	const investMin = parseAmount(filters.investMin);
	const investMax = parseAmount(filters.investMax);
	const minRoiPct = parseAmount(filters.minRoiPct);
	const minGain = parseAmount(filters.minGain);

	return plays.filter((play) => {
		const investment = play.investment * quantity;
		if (investMin !== null && investment < investMin * scale) return false;
		if (investMax !== null && investment > investMax * scale) return false;
		if (minRoiPct !== null && play.roiPct < minRoiPct / PERCENT_PER_FRACTION) return false;
		if (minGain !== null && play.roi * quantity < minGain) return false;
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
 */
export function overDepth(play: CurrencyExchangePlay, quantity: number): boolean {
	return quantity > play.depth;
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
