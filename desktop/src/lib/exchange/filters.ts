/**
 * The Currency Exchange page's client-side filtering (POE-186).
 *
 * The server ranks; this file only narrows what the reader looks at. Every
 * predicate reads a wire field, one derived figure, or an id it has to recognise
 * — `runInvestment` is the Run cost bound's figure and `DIVINE_ID` is what the
 * divine trash-price gate scopes itself by, and those two are this file's whole
 * runtime dependency on `./view` — and nothing here is fetched: the filters run
 * over the list already on screen, so a rule that empties the table is a rule
 * the reader can undo without a round trip.
 *
 * Two layers of rules, one verdict. A category rule paints all sixteen sidebar
 * groups; an item rule names one exchange id and beats the category it belongs
 * to. Both live in `persisted()` strings (ADR-013), which is why the parsers
 * take raw text and answer with an empty rule set rather than throwing.
 *
 * Two ways of reading an empty input live here, and since POE-193 they agree on
 * the answer without agreeing on the reasoning. The quality gates (POE-191,
 * moved out of the server) ship OFF with TWO sanctioned exceptions: an unset
 * knob is a gate the reader has not armed, and `gateDefaults` is the one place a
 * build says what unset runs at — five zeros, `minItemPrice` at 0.5 and
 * `minItemPriceDiv` at 0.4, the two trash-price knobs ADR-017 names as the only
 * default-on filters on this side (POE-196; the divine twin is the owner ruling
 * of 2026-08-23). The numeric bounds are off
 * by construction: they have no default to consult, because an unset bound is a
 * filter the reader is not running.
 * `parseGate` and `parseAmount` each say what their side does with input that is
 * NOT blank — that is where the two still differ, so check which one you are in
 * before reusing either.
 *
 * Pure TypeScript on purpose — no Svelte runes, no Tauri imports — so the whole
 * file is reachable from vitest without a component harness, the same reason
 * `view.ts` gives.
 */
import type { CurrencyExchangePlay } from '$lib/api';
import { DIVINE_ID, runInvestment } from './view';
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
 * The seven quality gates, parsed.
 *
 * Five of them are the floors the SERVER used to apply before it served anything
 * (POE-191 moved them here: the server now serves everything sane — liveness,
 * persistence, positivity, cap — and the reader owns the quality bar). The other
 * two, `minItemPrice` and `minItemPriceDiv`, never had a server side: they are
 * the trash-price knobs POE-196 and the owner ruling of 2026-08-23 added, one
 * per entry denomination. Nothing here re-derives a market number either; every
 * gate reads a field the wire already carries per play — the divine knob
 * un-multiplies one by the response's own rate, which is the inverse of the
 * server's own conversion and not a second market reading (`applyGates`).
 *
 * `0` is off for every knob, `maxTickPct` included: a spread ceiling of zero
 * would ask for a market with no spread at all, which is not a table anyone
 * wants, so the value is free to spell "no ceiling". `applyGates` spells that
 * out as a `> 0` guard per comparison, which is what lets five of the seven ship
 * at 0 and lets the other two be turned off by a reader who types 0 into them.
 */
export interface Gates {
	/**
	 * ENTRY price of one exchange in CHAOS, floor — the chaos trash-price knob
	 * (POE-196), one of the two gates that ship armed. Judged on `investment`,
	 * what ONE exchange costs at the undercut entry, which is the price of the
	 * thing the play is made of; at the shipped 0.5 it removes the bottom of the
	 * sub-chaos tier.
	 *
	 * It judges EVERY play, whatever the entry quote is, because `investment` is
	 * always chaos. The divine knob below is the narrower of the two and does not
	 * displace it: a divine-quoted play answers both.
	 */
	minItemPrice: number;
	/**
	 * ENTRY price of one exchange in DIVINE, floor, on DIVINE-QUOTED plays only —
	 * the divine twin of the knob above (owner ruling, Sebastian, 2026-08-23) and
	 * the second gate that ships armed.
	 *
	 * Scoped by the entry quote and not by size: a play whose buy leg quotes in
	 * chaos is never judged here at all, however cheap it is, because the chaos
	 * knob is already the whole of that market's trash line. Judged on the same
	 * figure `minItemPrice` judges, read back into divine — `investment` divided
	 * by the response's `divineChaosRate`, which is the rate the server valued
	 * that `investment` with. At the shipped 0.4 it removes the
	 * hundreds-per-divine tier, where an item is worth a fraction of an orb each
	 * and the undercutting is a war over a price step.
	 */
	minItemPriceDiv: number;
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
 * What an unset knob runs at: nothing, for five of the seven (POE-193). The
 * other two are the trash-price pair — `minItemPrice` at 0.5 chaos (POE-196) and
 * `minItemPriceDiv` at 0.4 divine (owner ruling, 2026-08-23) — the only
 * default-on filters this side ships, and the only sanctioned exceptions to
 * ADR-017's visibility rule.
 *
 * The exception is narrow on purpose and is ONE argument applied twice. ADR-017
 * bars a DEFAULT from hiding a live market, and the markets these hide are live:
 * Clear Oil and its neighbours clear at a fraction of a chaos all day, and the
 * hundreds-per-divine tier clears all day too. The claim is not that those
 * markets are fake — nothing here reads `tick`, and a cheap market can quote as
 * finely as any other. It is about ABSOLUTE SIZE: an entry the reader cannot act
 * on at any repeat count they would actually sit through is noise in front of
 * the ones they can. That is a judgement about a reader, which is exactly the
 * kind of thing ADR-015 puts on this side of the line and exactly the kind of
 * thing that has to be visible and undoable: both knobs are on the Gates row
 * with the other five, their placeholders show the shipped level rather than
 * `off`, and typing 0 turns either off for good.
 *
 * The two are separate knobs and not one converted level, because the size they
 * judge is not one size. At the ~200c divine rate this repo's recorded fixtures
 * carry, 0.4 div is about 80 chaos: a single chaos-denominated floor set there
 * would take out most of the table, and a single divine-denominated one set at
 * the 0.5c equivalent would be 0.0025 div and would never fire. The reader's
 * question is different per denomination too:
 * chaos rows are trash below half an orb, divine rows are trash when it takes
 * hundreds of the item to make an orb. Two lines, two markets, and every play is
 * governed by exactly the one its entry quote names.
 *
 * The levels are the owner's calls. 0.5 rather than a round 1 (2026-08-22)
 * because ADR-015's own motivating example was Sacrifice fragments at ~0.2–1c
 * and a 1c floor would have hidden the flips that ADR was written to rescue: at
 * 0.5 the fragment and oil tier stays visible from half a chaos up and only the
 * bottom of it goes. 0.4 (2026-08-23) because an item priced in REAL fractional
 * divines is interesting from about four tenths of an orb each, and below that
 * the divine side of the exchange is hundreds-per-divine junk.
 *
 * Data rather than seven literals spread through the parsers, because four
 * callers need the same seven numbers and they must not be able to disagree:
 * the parser's fallback, `resetGateInputs`, `movedGates`, and the filter bar's
 * placeholders.
 *
 * Until POE-193 the five carried the levels the server used to enforce for
 * everyone (3 / 10,000 / 10% / 5× / 2%) and shipped them armed, so the
 * out-of-the-box table was the old server's table. The rule now is that
 * everything the server serves above the trash tier is VISIBLE out of the box,
 * with the two trash-price knobs the whole of that qualifier, because the judgement
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
 *
 * Neither trash-price knob has such a second language: neither ever existed
 * server-side and nothing in Go knows about either, so this constant is their
 * only owner.
 */
export const gateDefaults: Gates = {
	minItemPrice: 0.5,
	minItemPriceDiv: 0.4,
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
 * 0 — off — for five of the seven and, for the two trash-price knobs, 0.5 chaos
 * (`minItemPrice`, POE-196) and 0.4 divine (`minItemPriceDiv`, 2026-08-23).
 * That reaches the same place `parseAmount` reaches by a different route for the
 * five, and the route is the part worth keeping: this side asks a BUILD what an
 * unset knob runs at, the other side has no such value to ask for. The other two
 * are what make the difference load-bearing rather than academic — hard-coding 0
 * here would ship the trash tier to every reader who never typed a level. The
 * knobs persist as `''` precisely so a build that changes what unset means
 * reaches the reader who never touched one, instead of leaving them on a number
 * an older build wrote into their settings file.
 *
 * `0` still has to survive parsing rather than reading as unset: a reader who
 * typed it is saying "show me the cheap fragments too", and the box they typed
 * it into has to keep saying so. That is the only way to disarm either
 * trash-price knob, which is why the two readings of an empty box must not be
 * allowed to merge — blanking either box restores that build's shipped floor.
 *
 * A negative knob is read as off rather than kept, and the reason is the
 * server's arming convention, not the plays: since the MinEdge demotion (ADR-017
 * amendment, 2026-08-22) a served play CAN carry a negative `roiPct` or `roi` —
 * a spreadless hour is served flagged `lowLiquidity` with its measured loss —
 * so a floor below zero could genuinely drop rows. The server treats 0 and
 * below as "unset" on its own gates for the same knobs, and a client that let a
 * stored negative fire would remove rows the server deliberately serves; "off"
 * keeps the two ends of the contract saying the same thing. Both trash-price
 * knobs judge `investment`, what an exchange COSTS — one in chaos and one
 * divided back into divine — which is never negative. For
 * the one ceiling (maxTickPct) a negative would fire against EVERY play and
 * empty the table; a stored negative there is garbage, not a request, and off
 * is the recovery. (`expectedRoi` has been free to go negative since POE-193 —
 * ADR-016 serves and flags the measured losers; no gate here reads it.)
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
		minItemPrice: parseGate(inputs.minItemPrice, gateDefaults.minItemPrice),
		minItemPriceDiv: parseGate(inputs.minItemPriceDiv, gateDefaults.minItemPriceDiv),
		minRoiChaos: parseGate(inputs.minRoiChaos, gateDefaults.minRoiChaos),
		minTurnover: parseGate(inputs.minTurnover, gateDefaults.minTurnover),
		maxTickPct: parseGate(inputs.maxTickPct, gateDefaults.maxTickPct),
		minEdgeTickRatio: parseGate(inputs.minEdgeTickRatio, gateDefaults.minEdgeTickRatio),
		minRoiPct: parseGate(inputs.minRoiPct, gateDefaults.minRoiPct)
	};
}

/**
 * What a gate box should hold once the reader has left it — or `null` for "leave
 * what they typed alone".
 *
 * The bar snaps a box on blur so it cannot sit there showing something other than
 * what the gate is running at: `abc` on screen while the shipped floor filters is
 * a control lying about itself. Mid-typing is exempt by construction — the snap
 * happens on blur, so `1` on its way to `10` is never fought — and a blank box is
 * left blank, because `''` is how a knob says "whatever this build ships" and
 * writing anything into it would be the reset's opposite.
 *
 * The un-parseable case answers `''` and NOT the default's digits, and that is the
 * whole reason this is a function rather than a line in the handler. `parseGate`
 * resolves `abc` to the knob's default, and a snap that wrote `String(...)` of it
 * would put `0.4` in the box and in the settings file — the knob would then be
 * FROZEN on this build's number, and a build that moved the floor would never
 * reach that reader again. That is the POE-191 freeze the empty-string persistence
 * exists to prevent, and it only became reachable when a default stopped being 0:
 * `'0'` in the box is a state a reader can also mean, but `'0.4'` is one nobody
 * asked for. Blank is the same VALUE the default resolves to and the same state
 * the box started in.
 *
 * A parseable number still snaps to its parsed form, `-5` → `'0'` included: a
 * negative reads as the gate turned off (`parseGate`), and off is a state the
 * reader can be shown honestly.
 *
 * Here rather than inline in `ExchangeFilterBar.svelte` for the reason this whole
 * file is here — a `.svelte` file has no unit-test harness in this repo, and the
 * decision is one `parseGate` is half of.
 */
export function snapGateInput(raw: string, fallback: number): string | null {
	if (raw.trim() === '') return null;
	if (!Number.isFinite(Number(raw))) return '';
	const snapped = String(parseGate(raw, fallback));
	return snapped === raw ? null : snapped;
}

/**
 * The Gates row's Defaults button, as stored state: every knob back to `''`.
 *
 * Blank and never the number it stands for. `parseGate` reads unset as the
 * BUILD's default, so emptying the boxes IS the reset and it leaves the reader
 * on whatever this build ships rather than on a snapshot of it. POE-196 made
 * that difference observable: for the five gates that ship off, a reset writing
 * `'0'` would be indistinguishable from this one, and it would permanently
 * disable the trash-price floors while looking correct — both of them, since the
 * divine twin joined them on 2026-08-23.
 *
 * A function here rather than a loop inside the page's `resetGates`, so the
 * contract has a seam a test can hold — the page writes these strings into the
 * preferences and does nothing else. Keyed off `gateDefaults` for the reason
 * `movedGates` is: a knob added to the type cannot be left out of the reset.
 */
export function resetGateInputs(): GateInputs {
	const knobs = Object.keys(gateDefaults) as (keyof Gates)[];
	return Object.fromEntries(knobs.map((knob) => [knob, ''])) as GateInputs;
}

/**
 * Which knobs sit somewhere other than where this build ships them — what the
 * filter bar badges its collapsed Gates row with.
 *
 * MOVED, not armed. The two were the same fact while every default was 0 and
 * stopped being one in POE-196: a reader who types 0 into either trash-price
 * knob has turned a shipped floor OFF and is counted here, which is what the
 * badge is for — it says the table is not showing this build's answer, and
 * disarming a default changes that as surely as arming a knob does.
 *
 * Compared on the PARSED values, never on the strings: '', '0' and '0.0' are one
 * gate at 0, and a count over text would call two of them a change. Keyed off
 * `gateDefaults` rather than off the bar's field list so a knob cannot be added
 * to the type and quietly go uncounted.
 *
 * Pure and here rather than a `$derived` in the bar, because it is the third
 * caller `gateDefaults` exists to keep in agreement (the parser's fallback and
 * the Defaults reset are the other two) and the one of the three whose answer is
 * worth asserting on its own.
 */
export function movedGates(gates: Gates): (keyof Gates)[] {
	const knobs = Object.keys(gateDefaults) as (keyof Gates)[];
	return knobs.filter((knob) => gates[knob] !== gateDefaults[knob]);
}

/**
 * The play's ENTRY price for one exchange, in DIVINE — or `null` when this play
 * is not the divine gate's business.
 *
 * `null` has exactly two shapes and both mean "do not judge", never "fail":
 * 1. The entry quote is not divine. The gate is scoped by the buy leg's `quote`,
 *    which is the currency the reader spends to get in, and `legs[0]` is that leg
 *    on a direct play and on a 1-hop alike (`view.ts`'s `runLedger` and
 *    `routeSlots` read the same position for the same reason). A chaos-quoted
 *    play is governed by `minItemPrice` alone, and a play with no legs at all —
 *    not a served shape — is nobody's business here.
 * 2. The response carried no usable `divineChaosRate`. That is 0 only in an hour
 *    with no divine/chaos trade, in which case no divine-quoted play is served at
 *    all (`api.ts`, `CurrencyExchangeResponse.divineChaosRate`), so this arm is a
 *    guard rather than a case — and the guard answers "unjudged" rather than
 *    dividing by zero and comparing an `Infinity` against a floor.
 *
 * THE FIGURE, AND WHY IT IS THIS ONE. `investment / divineChaosRate` and not the
 * buy leg's own `price`, even though that price is already divine-denominated
 * and would need no rate. Three reasons, in order of weight:
 *
 * - It is the SAME FIGURE the chaos twin judges, in the other denomination. The
 *   two knobs are one line drawn in two currencies, so they must not read two
 *   different prices: `investment` is the UNDERCUT entry — what the reader
 *   actually pays to be the order that fills — while a leg's `price` is the
 *   hour's printed EXTREME, one tick better than anything obtainable. A pair of
 *   twins disagreeing about a boundary by one tick is the kind of split this
 *   surface exists to prevent.
 * - The division is an UN-CONVERSION, not a second market reading. The server
 *   valued this play's `investment` in chaos with this response's own
 *   `divineChaosRate` (`api.ts`), so dividing it back recovers the divine figure
 *   the server started from rather than approximating it at some other rate.
 * - Something on the row PRINTS it. On a divine-quoted play whose buy market
 *   posts one at a time, the route's Spend slot is `investment / rate` exactly
 *   (`view.ts`'s `runLedger.spend` at `displayScale().units === 1`). A leg
 *   `price` appears on the row only inside the buy step's hover, as the market's
 *   posted quantity pair, and never as a per-item number.
 *
 * Deliberately NOT `price × (1 + tick)`, which would rebuild the undercut from
 * the leg and need no rate either: that is re-deriving a market number, which is
 * the one thing this file's header promises it does not do.
 */
function divineEntryPrice(play: CurrencyExchangePlay, divineChaosRate: number): number | null {
	if (play.legs[0]?.quote !== DIVINE_ID) return null;
	if (!(divineChaosRate > 0)) return null;
	return play.investment / divineChaosRate;
}

/**
 * Apply the quality gates.
 *
 * Its own function rather than seven more fields on `applyNumericFilters`,
 * because the two halves are set and unset by different controls: the gates are
 * a standing quality bar with their own Defaults reset and their own explicit
 * off (`0`), the bounds are a question the reader asked once and Clear sweeps
 * up. Folding them together would put both contracts inside one filter pass
 * where the next reader has to remember which field obeys which — and the page
 * reflects the same split, since Clear empties the bounds and leaves the gates
 * where the reader put them.
 *
 * `divineChaosRate` is the response's own newest-hour rate and the only market
 * datum this pass takes beyond the plays. It is a parameter and not a `Gates`
 * field on purpose: `Gates` is what the READER set and what `movedGates` badges
 * against `gateDefaults`, and folding a response value into it would make the
 * badge fire every time the exchange moved.
 *
 * A play passes only by clearing every gate that is on:
 * - `investment ≥ minItemPrice` — the ENTRY price of one exchange in chaos, the
 *   chaos trash-price knob (POE-196). `investment` and not a leg's `price`,
 *   because the wire's per-leg price is quoted in whatever that leg trades
 *   against and a 1-hop route has three of them: `investment` is the one figure
 *   that is always chaos and always the cost of entering, so a 1-hop entry is
 *   judged by the same number as a direct one with nothing special-cased. Never
 *   the SCALED investment either — that is the run's size, which is the Run cost
 *   bounds' question, and a knob about how cheap the ITEM is must not answer it.
 * - `investment / divineChaosRate ≥ minItemPriceDiv`, on DIVINE-QUOTED ENTRIES
 *   ONLY — the divine trash-price knob (owner ruling, 2026-08-23), the same
 *   figure as the line above read back into the currency the reader is spending.
 *   `divineEntryPrice` owns both the scoping and the choice of figure. A
 *   chaos-quoted play is not judged here at all: the two knobs are not a
 *   sequence a play walks down but one line per denomination, so hundreds-per-
 *   divine junk is dropped by this one and sub-half-chaos junk by the one above,
 *   and neither reaches into the other's market. A divine-quoted play answers
 *   BOTH — the chaos knob reads every play, `investment` always being chaos —
 *   which costs nothing in practice, since 0.4 div is far above 0.5c at any rate
 *   the exchange has carried.
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
 * A gate at 0 does not run at all, which is what makes 0 the shipped default of
 * five of them (`gateDefaults`) as well as every reader's explicit off — the
 * `> 0` guard is the off switch, one per comparison, `maxTickPct` included and
 * both trash-price knobs included, those last two being how a reader disarms the
 * gates that ship armed. Each disarms alone: typing 0 into the chaos knob leaves
 * the divine floor standing, and the reverse.
 *
 * Every comparison is inclusive on the passing side: a play sitting exactly on
 * a floor met it, and the server these numbers were taken from cleared the same
 * plays when it enforced them. Both trash-price knobs follow the same side — an
 * exchange entered at exactly 0.5c, or at exactly 0.4 div, is kept at the
 * shipped level, and only a play strictly under is dropped — so the seven knobs
 * answer a boundary the same way and no reader has to remember which one is the
 * exception.
 *
 * `maxTickPct` and `minRoiPct` cross scales here and nowhere else — the inputs
 * are percent points, the wire's `tick` and `roiPct` are fractions.
 * `minEdgeTickRatio` does not cross anything: a bare ratio times a fraction is
 * already a fraction.
 *
 * Every gate that reads a RETURN judges the BEST-CASE `roiPct`/`roi`,
 * deliberately, even though the server now ranks on `expectedRoi` (POE-193). The
 * levels a reader arms are the server's old ones and were calibrated against
 * those two fields; the Go test that arms them measures the same numbers, so
 * re-pointing a gate at the expectation would silently change what a typed level
 * cuts. What the expectation is for is the ORDER and the Exp. ROI column — the
 * reader sees the measured outcome and decides, which is ADR-015's split with
 * the bar still on the reader's side, and since POE-193 with nothing on that bar
 * beyond the trash tier until they put it there. Neither trash-price knob reads
 * a return at all — both are facts about the price of the thing, not about what
 * the thing pays — so the choice does not arise for either.
 */
export function applyGates(
	plays: CurrencyExchangePlay[],
	gates: Gates,
	divineChaosRate: number
): CurrencyExchangePlay[] {
	return plays.filter((play) => {
		if (gates.minItemPrice > 0 && play.investment < gates.minItemPrice) return false;
		if (gates.minItemPriceDiv > 0) {
			const entryDiv = divineEntryPrice(play, divineChaosRate);
			if (entryDiv !== null && entryDiv < gates.minItemPriceDiv) return false;
		}
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
 * THE SEAM, STATED. The bound reads `runInvestment(play)` — the RUN's cost —
 * and the Investment column reads `moneyColumns(play).investment`, which since
 * the owner rulings of 2026-08-22 is the DISPLAY scale: one posting of the buy
 * market, on every row and in every entry currency. The two are different
 * questions now, and the divergence is deliberate rather than accidental —
 * which is why this reads a function named for the run instead of the one the
 * column reads (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §6.1, "the
 * exception's own exception").
 *
 * Left pointing at `moneyColumns` the bound would have become a per-posting
 * ceiling the moment the display scale moved, silently: the code would still
 * have compiled, and a reader typing 500 to mean "500c of bankroll" would have
 * been filtering on what one trash-market order costs. A bankroll ceiling is a
 * run-sized question whatever the row prints, so the bound follows the meaning
 * and not the column.
 *
 * What that costs is a real reading: the number this compares against is
 * printed in the SCALE column ("N c in") and not in the Investment column
 * beside it. That is the same shape §6.1's per-exchange gates already carry —
 * a control whose figure lives elsewhere on the row — and the Run cost tooltip
 * names the Scale column for it.
 *
 * A play whose scale cannot be derived falls back to its per-exchange
 * investment rather than being dropped or waved through — that fallback lives
 * inside `runInvestment`. Since POE-193 that
 * condition is `expectedRoi ≤ 0`, and it is a case the table HITS: the
 * server's positivity floor governs the best-case `roi`, while the simulated
 * expectation is free to measure a loss and the play is served anyway — 7.9% of
 * the calibration set realized negative (ADR-016). Falling back is therefore a
 * deliberate choice about a live case and not a guard against an impossible
 * one: dropping such a play here would be an eighth quality gate, hidden inside a
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
		const investment = runInvestment(play);
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
