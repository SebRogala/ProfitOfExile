/**
 * Harvest Flipping view projection — everything the page shows, as data.
 *
 * Pure: the two owners' read models, the player's picks and the clock in; a
 * page model out (state, tabs, status line, verdict, tier cards, regex boxes,
 * cost and EV panels). Components render these strings and flags and decide
 * nothing, so every label, number and state is asserted here, unmounted.
 *
 * Labels come from the read models (family labels, tier names, lifeforce
 * names and colours); the only constants are the reference screens' copy.
 * The divine rate is the caller's (D5: one rate for the status line and the
 * divine-scale line), never read from the exchange model here.
 */
import {
	HORIZON_OPTIONS,
	currencyIconPath,
	deriveState,
	formatTime,
	formatTimeAgo,
	type CurrencyExchangeHorizon,
	type ViewState,
	type ViewStateKind
} from '$lib/exchange/view';
import {
	DIVINE_LINE_MIN_EV_CHAOS,
	familyTypes,
	solveFamily,
	type FamilyResult,
	type Pick
} from './engine';
import { STASH_SEARCH_LIMIT, stashRegex } from './regex';
import type { ExchangeItemPrice, ExchangePriceRead, HarvestFamily, HarvestFamilyData } from './seam';

// ------------------------------------------------------------- formatting --

const MINUS = '−';
const DASH = '—';

/**
 * Half-up rounding to `decimals` places on a decimal-scaled value: the scaled
 * magnitude is cut to 12 significant digits before `Math.round`, so a tie that
 * binary floating point stores a hair low (19.35 → 193.49999999999997) still
 * rounds up. Rounded on the magnitude and signed after, so a loss and the
 * matching gain round alike.
 */
function roundHalfUp(value: number, decimals: number): number {
	const scale = 10 ** decimals;
	const scaled = Number((Math.abs(value) * scale).toPrecision(12));
	return (Math.sign(value) * Math.round(scaled)) / scale;
}

/** Comma grouping by hand (as `exchange/view.ts` `formatChaos`): a locale grouping with "." would misread. */
function group(whole: number): string {
	return String(whole).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** `_formats.chaos`: one decimal below 100 (`9.1c`), whole and grouped from 100 (`1,344c`). */
export function formatHarvestChaos(chaos: number): string {
	if (!Number.isFinite(chaos)) return DASH;
	const sign = chaos < 0 ? MINUS : '';
	const tenth = roundHalfUp(Math.abs(chaos), 1);
	if (tenth < 100) return tenth === 0 ? '0.0c' : `${sign}${tenth.toFixed(1)}c`;
	return `${sign}${group(roundHalfUp(Math.abs(chaos), 0))}c`;
}

/** `_formats.chaos` gains: `+24.3c`, `−0.3c` (U+2212); `|x| ≤ 0.05` prints unsigned. */
export function formatHarvestGain(chaos: number): string {
	if (!Number.isFinite(chaos)) return DASH;
	if (Math.abs(chaos) <= 0.05) return formatHarvestChaos(Math.abs(chaos));
	return chaos > 0 ? `+${formatHarvestChaos(chaos)}` : formatHarvestChaos(chaos);
}

/** `_formats.percent`: one decimal (`15.7%`), two below 1% (`0.11%`). Input is already a percentage. */
export function formatHarvestPercent(percent: number): string {
	if (!Number.isFinite(percent)) return DASH;
	return Math.abs(percent) < 1
		? `${roundHalfUp(percent, 2).toFixed(2)}%`
		: `${roundHalfUp(percent, 1).toFixed(1)}%`;
}

/** `_formats.rolls`: `~6.0`. */
export function formatHarvestRolls(rolls: number): string {
	if (!Number.isFinite(rolls)) return DASH;
	return `~${roundHalfUp(rolls, 1).toFixed(1)}`;
}

/** `_formats.counts`: `9,905`. */
export function formatHarvestCount(count: number): string {
	if (!Number.isFinite(count)) return DASH;
	return group(roundHalfUp(count, 0));
}

// ------------------------------------------------------------------ prefs --

/** Picks per family id, then per itemId. */
export type HarvestPicks = Record<string, Record<string, Pick>>;
type Prices = Record<string, ExchangeItemPrice>;

function isPick(value: unknown): value is Pick {
	return value === 'keep' || value === 'reroll';
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function priceOf(prices: Prices, itemId: string): number | null {
	if (!Object.hasOwn(prices, itemId)) return null;
	const chaos = prices[itemId].chaos;
	return typeof chaos === 'number' && Number.isFinite(chaos) ? chaos : null;
}

/**
 * Read the `harvestFlippingPicks` pref. Garbage becomes `{}`; a family id or
 * itemId the families do not carry is dropped (own keys only, via `Map`, so a
 * `constructor` or `__proto__` key is just an unknown key); a family left
 * with no pick is dropped.
 */
export function parsePicks(raw: string, families: HarvestFamily[]): HarvestPicks {
	let parsed: unknown;
	try {
		parsed = JSON.parse(raw);
	} catch {
		return {};
	}
	if (!isRecord(parsed)) return {};
	const byId = new Map(families.map((f) => [f.id, f]));
	const entries: [string, Record<string, Pick>][] = [];
	for (const familyId of Object.keys(parsed)) {
		const family = byId.get(familyId);
		const value = parsed[familyId];
		if (!family?.weights || !isRecord(value)) continue;
		const known = new Set(family.weights.types.map((t) => t.itemId));
		const picks = Object.keys(value)
			.filter((itemId) => known.has(itemId) && isPick(value[itemId]))
			.map((itemId) => [itemId, value[itemId] as Pick] as const);
		if (picks.length > 0) entries.push([familyId, Object.fromEntries(picks)]);
	}
	return Object.fromEntries(entries);
}

export function serializePicks(picks: HarvestPicks): string {
	return JSON.stringify(picks);
}

function withFamily(picks: HarvestPicks, familyId: string, next: Record<string, Pick>): HarvestPicks {
	const others = Object.entries(picks).filter(([id]) => id !== familyId);
	return Object.fromEntries(Object.keys(next).length > 0 ? [...others, [familyId, next]] : others);
}

function familyPicks(picks: HarvestPicks, familyId: string): Record<string, Pick> {
	return Object.hasOwn(picks, familyId) ? picks[familyId] : {};
}

/**
 * Set (`pick`) or clear (`null`) one type's pick in one family. An unpriced
 * type takes no pick (C2: it is always a keeper), so any pick on it is dropped.
 */
export function applyPick(
	picks: HarvestPicks,
	familyId: string,
	itemId: string,
	pick: Pick | null,
	prices: Prices
): HarvestPicks {
	const next = Object.fromEntries(Object.entries(familyPicks(picks, familyId)).filter(([id]) => id !== itemId));
	if (pick !== null && priceOf(prices, itemId) !== null) next[itemId] = pick;
	return withFamily(picks, familyId, next);
}

/** Pick every priced type of one tier to one side; unpriced types are skipped (C2). */
export function moveTier(
	picks: HarvestPicks,
	family: HarvestFamily,
	tier: string,
	pick: Pick,
	prices: Prices
): HarvestPicks {
	const byItem = family.tiers?.byItem ?? {};
	return (family.weights?.types ?? [])
		.filter((t) => Object.hasOwn(byItem, t.itemId) && byItem[t.itemId] === tier)
		.reduce((acc, t) => applyPick(acc, family.id, t.itemId, pick, prices), picks);
}

/** Reset to computed: clears the given family's picks only. */
export function resetFamily(picks: HarvestPicks, familyId: string): HarvestPicks {
	return withFamily(picks, familyId, {});
}

/** C3: Harvest's horizon falls back to `'day'` (the fixture's window), not CX's `'recent'`. */
export function parseHarvestHorizon(raw: string): CurrencyExchangeHorizon {
	const found = HORIZON_OPTIONS.find((o) => o.value === raw);
	return found ? found.value : 'day';
}

// ----------------------------------------------------------------- status --

export interface OwnerStateInput {
	exchange: ExchangePriceRead | null;
	harvest: HarvestFamilyData | null;
	lastFetchedAt: Date | null;
	lastError: string | null;
	now: Date;
}

/**
 * D8: CX's `deriveState` over both owners. A result exists only when both
 * owners answered; it is warm only when both are; its age is the exchange's.
 * So an error with both on screen is `stale`, an error before both arrived is
 * `unreachable`, and either owner cold is `warming`.
 */
export function combineOwnerState({ exchange, harvest, lastFetchedAt, lastError, now }: OwnerStateInput): ViewState {
	const result =
		exchange && harvest ? { warm: exchange.warm && harvest.warm, lastUpdated: exchange.lastUpdated } : null;
	return deriveState({ result, lastFetchedAt, lastError, now });
}

export interface StatusLine {
	segments: string[];
	text: string;
	stale: boolean;
}

const NUMBER_WORDS = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten'];

function countWord(n: number): string {
	return NUMBER_WORDS[n] ?? formatHarvestCount(n);
}

function horizonLabel(horizon: CurrencyExchangeHorizon): string {
	return HORIZON_OPTIONS.find((o) => o.value === horizon)?.label ?? horizon;
}

/** Reference 06: the status line of a family without logged weights. */
const WEIGHTS_NOT_LOGGED = 'weights: no log yet';

function weightsSegment(family: HarvestFamily): string {
	const weights = family.weights;
	if (!weights) return WEIGHTS_NOT_LOGGED;
	const { sample, types } = weights;
	if (sample.rolls === null) {
		return `weights: no logged sample · ${sample.uniform ? 'assumed uniform, ' : ''}closed pool of ${countWord(types.length)}`;
	}
	const kind = sample.uniform ? `uniform ~${Math.round(100 / types.length)}%` : "one player's sample";
	return ['weights: HarvestForge log', `${formatHarvestCount(sample.rolls)} rolls`, sample.date, kind]
		.filter((s): s is string => s !== null)
		.join(' · ');
}

/** The status line under the tabs (references 01, 04, 08 §3). Shown once a result is on screen. */
export function statusLine(
	state: ViewState,
	exchange: ExchangePriceRead,
	family: HarvestFamily,
	horizon: CurrencyExchangeHorizon,
	divineChaosRate: number | null,
	lastFetchedAt: Date | null,
	now: Date
): StatusLine {
	const segments: string[] = [];
	const stale = state.kind === 'stale';
	if (stale) {
		segments.push(`stale since ${formatTime((lastFetchedAt ?? now).toISOString())} — server unreachable`);
		if (exchange.lastUpdated !== null) {
			segments.push(
				`prices from ${formatTime(exchange.lastUpdated)} (${formatTimeAgo(exchange.lastUpdated, now)})`
			);
		}
	} else if (state.updatedAgo) {
		segments.push(`updated ${state.updatedAgo}`);
	}
	const rate = divineChaosRate === null ? DASH : `${formatHarvestCount(divineChaosRate)}c`;
	segments.push(`prices: Currency Exchange, ${horizonLabel(horizon)} · 1 div = ${rate}`);
	segments.push(weightsSegment(family));
	return { segments, text: segments.join(' · '), stale };
}

// ------------------------------------------------------------------ cards --

export interface Chip {
	itemId: string;
	name: string;
	shortName: string;
	/** For `iconSrc(apiBase, icon)`. */
	icon: string;
	priceText: string;
	unpriced: boolean;
	picked: boolean;
	pickTitle: string | null;
	/** Feeders only: the loop EV, signed. */
	evText: string | null;
	evTone: 'gain' | 'loss' | 'flat' | null;
	/** Priced keepers only: the weight share. */
	shareText: string | null;
}

export interface TierCard {
	tier: string;
	split: boolean;
	/** `7 types · 30.0c–109c`, `1 type · 402c`, `1 type · —`. */
	meta: string;
	moveLabel: string;
	/** The side the move button sends the tier to. */
	moveTo: Pick;
	chips: Chip[];
}

export interface Column {
	cards: TierCard[];
	/** Types `tiers.byItem` does not place; listed rather than hidden (ADR-017). */
	untiered: Chip[];
	/** Replaces an empty column (reference 08 §7). */
	empty: string | null;
}

/** One type as the cards see it: its side, price, pick and tier. */
export interface Row {
	itemId: string;
	name: string;
	shortName: string;
	price: number | null;
	kept: boolean;
	picked: Pick | null;
	loopEv: number | null;
	share: number;
	tier: string | null;
}

const TOP_TIER = 'TOP';

/** Q4 (PENDING OPERATOR, P45): titles of a chip the player picked; README asks only for "a title that says so". */
export const PICK_TITLE_KEPT = 'Your pick: kept. Click to return it to the computed side.';
export const PICK_TITLE_FED = 'Your pick: fed. Click to return it to the computed side.';

function chipOf(row: Row): Chip {
	const tone =
		row.loopEv === null ? null : row.loopEv > 0.05 ? 'gain' : row.loopEv < -0.05 ? 'loss' : 'flat';
	return {
		itemId: row.itemId,
		name: row.name,
		shortName: row.shortName,
		icon: currencyIconPath(row.itemId),
		priceText: row.price === null ? DASH : formatHarvestChaos(row.price),
		unpriced: row.price === null,
		picked: row.picked !== null,
		pickTitle:
			row.picked === null
				? null
				: row.picked === 'keep'
					? PICK_TITLE_KEPT
					: PICK_TITLE_FED,
		evText: row.kept || row.loopEv === null ? null : formatHarvestGain(row.loopEv),
		evTone: row.kept ? null : tone,
		// Reference 08 §5 draws an unpriced keeper as "— UNPRICED" with no share.
		shareText: row.kept && row.price !== null ? formatHarvestPercent(row.share * 100) : null
	};
}

/** Price high to low; an unpriced chip last; ties by name. Compares, never subtracts. */
function byPriceDesc(a: Row, b: Row): number {
	if (a.price !== b.price) {
		if (a.price === null) return 1;
		if (b.price === null) return -1;
		return a.price > b.price ? -1 : 1;
	}
	return a.name < b.name ? -1 : a.name > b.name ? 1 : 0;
}

function rangeText(members: Row[]): string {
	const priced = members.map((m) => m.price).filter((p): p is number => p !== null);
	if (priced.length === 0) return DASH;
	const lo = Math.min(...priced);
	const hi = Math.max(...priced);
	return lo === hi ? formatHarvestChaos(lo) : `${formatHarvestChaos(lo)}–${formatHarvestChaos(hi)}`;
}

/**
 * D11 + C1: one card per tier present on this side, TOP first then
 * `tiers.names` in order. The count and range cover the WHOLE tier (both
 * sides); a tier with types on both sides is marked split.
 */
export function tierCards(rows: Row[], tierNames: string[], side: 'feeders' | 'keepers'): TierCard[] {
	const kept = side === 'keepers';
	return [TOP_TIER, ...tierNames]
		.map((tier) => {
			const members = rows.filter((r) => r.tier === tier);
			const mine = members.filter((r) => r.kept === kept);
			if (mine.length === 0) return null;
			const n = members.length;
			return {
				tier,
				split: mine.length < n,
				meta: `${n} type${n === 1 ? '' : 's'} · ${rangeText(members)}`,
				moveLabel: kept ? 'Feed tier →' : '← Keep tier',
				moveTo: (kept ? 'reroll' : 'keep') as Pick,
				chips: [...mine].sort(byPriceDesc).map(chipOf)
			};
		})
		.filter((c): c is TierCard => c !== null);
}

// ---------------------------------------------------------------- verdict --

export interface Verdict {
	question: string;
	headline: string;
	reason: string | null;
	worth: boolean;
	evText: string | null;
	evNegative: boolean;
	per: string | null;
	divineLine: string | null;
}

const QUESTION = 'Is it worth it?';

/** The verdict panel per engine result (references 01, 08 §6–7). */
export function verdictView(result: FamilyResult, lifeforceName: string): Verdict {
	if (result.kind === 'ok') {
		const h = result.headline;
		const worth = h.loopEvChaos > DIVINE_LINE_MIN_EV_CHAOS;
		const line = result.divineLine;
		return {
			question: QUESTION,
			headline: worth ? 'Yes — feed the cheap tiers' : 'No — not at these prices',
			reason: worth
				? `Buy ${h.name} (${formatHarvestChaos(h.priceChaos)}, cheapest feeder) and reroll until a keeper.`
				: 'Even the cheapest feeder sells for more than the loop returns.',
			worth,
			evText: formatHarvestGain(h.loopEvChaos),
			evNegative: h.loopEvChaos < -0.05,
			per: `per ${h.name}`,
			divineLine:
				worth && line
					? `1 div profit ≈ ${formatHarvestCount(line.feeders)} feeders · ~${formatHarvestCount(line.rerolls)} rerolls · ~${formatHarvestCount(line.lifeforce)} ${lifeforceName}`
					: null
		};
	}
	return {
		question: QUESTION,
		headline: 'No — nothing to flip',
		reason: result.kind === 'no-keepers' ? 'The keep set is empty: pick a tier to keep.' : null,
		worth: false,
		evText: null,
		evNegative: false,
		per: null,
		divineLine: null
	};
}

// ------------------------------------------------------------ cost and EV --

export interface CostPanel {
	costText: string;
	perReroll: number;
	lifeforceIcon: string;
	/** `Wild lifeforce (purple)`. */
	lifeforceLabel: string;
	/** `0.9c = 30 × Wild lifeforce (purple)`; the page draws the icon between `×` and the label. */
	line: string;
	perDivine: string;
	caption: string;
}

export function costPanel(family: HarvestFamily, exchange: ExchangePriceRead): CostPanel | null {
	if (!family.lifeforce || family.rerollCost === null) return null;
	const lifeforce = exchange.lifeforce[family.lifeforce];
	const cost = family.rerollCost * lifeforce.chaos;
	const label = `${family.lifeforce} lifeforce (${lifeforce.colour})`;
	return {
		costText: formatHarvestChaos(cost),
		perReroll: family.rerollCost,
		lifeforceIcon: currencyIconPath(lifeforce.itemId),
		lifeforceLabel: label,
		line: `${formatHarvestChaos(cost)} = ${family.rerollCost} × ${label}`,
		perDivine: `1 div → ${formatHarvestCount(lifeforce.perDivine)} lifeforce → ${formatHarvestCount(Math.floor(lifeforce.perDivine / family.rerollCost))} rerolls`,
		caption: `chaos side ${lifeforce.chaos}c each; divine side from the lifeforce/divine market`
	};
}

export interface EvRow {
	label: string;
	value: string;
}

export interface EvPanel {
	rows: EvRow[];
	yield: EvRow[];
	/** Reference 08 §5: the floor note, when a type is unpriced. */
	unpricedNote: string | null;
}

export function evPanel(
	result: FamilyResult,
	lifeforceName: string,
	lastSeen: Map<string, number | null>
): EvPanel | null {
	if (result.kind !== 'ok') return null;
	const h = result.headline;
	let unpricedNote: string | null = null;
	const n = result.unpriced.length;
	if (n > 0) {
		unpricedNote = `${n} unpriced type${n === 1 ? '' : 's'} counted as 0c, so this EV is a floor.`;
		if (h.loopEvAtLastSeenChaos !== null) {
			const seen = n === 1 ? lastSeen.get(result.unpriced[0]) : null;
			unpricedNote +=
				n === 1 && typeof seen === 'number'
					? ` At its last seen ${formatHarvestChaos(seen)} it would read ${formatHarvestGain(h.loopEvAtLastSeenChaos)}.`
					: ` At their last seen prices it would read ${formatHarvestGain(h.loopEvAtLastSeenChaos)}.`;
		}
	}
	return {
		rows: [
			{ label: 'Cheapest feeder', value: `${h.name} · ${formatHarvestChaos(h.priceChaos)}` },
			{ label: 'Loop EV per feeder', value: formatHarvestGain(h.loopEvChaos) },
			{ label: 'Keeper hit per roll', value: formatHarvestPercent(h.keeperHitPerRoll * 100) },
			{ label: 'Rerolls per keeper', value: formatHarvestRolls(h.rerollsPerKeeper) },
			{
				label: 'Lifeforce per keeper',
				value: `~${formatHarvestCount(h.lifeforcePerKeeper)} ${lifeforceName} · ${formatHarvestChaos(h.lifeforceChaosPerKeeper)}`
			}
		],
		yield: [
			{ label: '1 div of lifeforce yields', value: `${formatHarvestRolls(h.keepersPerDivine)} keepers` },
			{ label: '…worth after inputs', value: formatHarvestGain(h.keepersPerDivineWorthChaos) }
		],
		unpricedNote
	};
}

// ------------------------------------------------------------------- page --

export interface RegexBox {
	text: string;
	length: number;
	/** `29 / 250 characters`, or `263 / 250 characters: too long for one stash search`. */
	count: string;
	overLimit: boolean;
}

export interface SplitRow {
	label: string;
	moved: boolean;
	resetEnabled: boolean;
}

export interface FamilyBody {
	kind: 'family';
	verdict: Verdict;
	split: SplitRow;
	feeders: Column;
	keepers: Column;
	regex: { feeders: RegexBox; keepers: RegexBox; caption: string };
	cost: CostPanel;
	ev: EvPanel | null;
}

export type PageBody =
	| FamilyBody
	| { kind: 'cold'; title: string; text: string }
	| { kind: 'no-data'; title: string; text: string };

export interface Tab {
	id: string;
	label: string;
	noData: boolean;
	active: boolean;
}

export interface PageInput extends OwnerStateInput {
	familyId: string;
	picks: HarvestPicks;
	horizon: CurrencyExchangeHorizon;
	/** D5: the one current divine rate (controller `currentDivineRate`). */
	divineChaosRate: number | null;
}

export interface PageView {
	state: ViewStateKind;
	/** The state's own line: `Loading…`, `Waiting for the first Currency Exchange hour…`, `Couldn't reach the server`. */
	stateLine: string | null;
	tabs: Tab[];
	status: StatusLine | null;
	body: PageBody | null;
}

const STATE_LINES: Partial<Record<ViewStateKind, string>> = {
	loading: 'Loading…',
	warming: 'Waiting for the first Currency Exchange hour…',
	unreachable: "Couldn't reach the server"
};

const REGEX_CAPTION =
	'Paste into the stash search. Shortest fragment unique within this family; follows your tier picks.';

/**
 * Q7 (PENDING OPERATOR, P45): reference 08 §9's no-data paragraph. Also shown
 * when the engine returns `no-data` for unusable numbers, where it misstates
 * the cause until a prices-unavailable variant is designed.
 */
export const NO_DATA_TEXT =
	'The EV needs how often each type comes out of a reroll. Nobody has sent a HarvestForge log for this family yet, so there is nothing to compute. The tab fills in when one arrives.';

function noDataBody(family: HarvestFamily): PageBody {
	return { kind: 'no-data', title: `${family.label}: reroll weights not logged yet`, text: NO_DATA_TEXT };
}

function regexBox(names: string[], familyNames: string[]): RegexBox {
	const r = stashRegex(names, familyNames);
	const count = `${r.length} / ${STASH_SEARCH_LIMIT} characters`;
	return {
		text: r.text,
		length: r.length,
		count: r.overLimit ? `${count}: too long for one stash search` : count,
		overLimit: r.overLimit
	};
}

function familyBody(input: PageInput, family: HarvestFamily, exchange: ExchangePriceRead): PageBody | null {
	const cost = costPanel(family, exchange);
	if (!family.weights || !family.lifeforce || !cost) return noDataBody(family);
	const lifeforce = exchange.lifeforce[family.lifeforce];
	const types = familyTypes(family.weights);
	const prices = new Map(types.map((t) => [t.itemId, priceOf(exchange.prices, t.itemId)]));
	const lastSeen = new Map(
		types.map((t) => [
			t.itemId,
			Object.hasOwn(exchange.prices, t.itemId) ? exchange.prices[t.itemId].lastSeenChaos : null
		])
	);
	const own = familyPicks(input.picks, family.id);
	const picks = new Map(
		types.filter((t) => Object.hasOwn(own, t.itemId) && isPick(own[t.itemId])).map((t) => [t.itemId, own[t.itemId]])
	);
	const result = solveFamily({
		types,
		prices,
		lastSeen,
		rerollCostChaos: (family.rerollCost ?? 0) * lifeforce.chaos,
		picks,
		lifeforcePerDivine: lifeforce.perDivine,
		divineChaosRate: input.divineChaosRate ?? 0
	});
	if (result.kind === 'no-data') return noDataBody(family);

	const totalWeight = types.reduce((s, t) => s + t.weight, 0);
	const byItem = family.tiers?.byItem ?? {};
	const solved = result.kind === 'ok' ? new Map(result.types.map((t) => [t.itemId, t])) : null;
	const rows: Row[] = types.map((t) => {
		const price = prices.get(t.itemId) ?? null;
		const s = solved?.get(t.itemId);
		const pick = price !== null ? (picks.get(t.itemId) ?? null) : null;
		return {
			itemId: t.itemId,
			name: t.name,
			shortName: t.shortName,
			price,
			kept: s ? s.kept : result.kind === 'no-feeders',
			picked: pick,
			loopEv: s ? s.loopEvChaos : null,
			share: s ? s.weightShare : t.weight / totalWeight,
			tier: Object.hasOwn(byItem, t.itemId) ? byItem[t.itemId] : null
		};
	});
	const tierNames = family.tiers?.names ?? [];
	const column = (side: 'feeders' | 'keepers'): Column => {
		const kept = side === 'keepers';
		const cards = tierCards(rows, tierNames, side);
		const untiered = rows.filter((r) => r.kept === kept && r.tier === null).sort(byPriceDesc).map(chipOf);
		const emptyText = kept ? 'No keepers: nothing to stop on.' : 'No feeders: every tier is kept.';
		return { cards, untiered, empty: cards.length + untiered.length === 0 ? emptyText : null };
	};
	const familyNames = types.map((t) => t.name);
	const moved = rows.filter((r) => r.picked !== null).length;
	return {
		kind: 'family',
		verdict: verdictView(result, family.lifeforce),
		split: {
			label: moved === 0 ? 'computed from prices' : `${moved} type${moved === 1 ? '' : 's'} moved by you`,
			moved: moved > 0,
			resetEnabled: moved > 0
		},
		feeders: column('feeders'),
		keepers: column('keepers'),
		regex: {
			feeders: regexBox(rows.filter((r) => !r.kept).map((r) => r.name), familyNames),
			keepers: regexBox(rows.filter((r) => r.kept).map((r) => r.name), familyNames),
			caption: REGEX_CAPTION
		},
		cost,
		ev: evPanel(result, family.lifeforce, lastSeen)
	};
}

/** The whole page model for one state (references 01–08). */
export function pageView(input: PageInput): PageView {
	const state = combineOwnerState(input);
	const { exchange, harvest } = input;
	const tabs: Tab[] = (harvest?.families ?? []).map((f) => ({
		id: f.id,
		label: f.label,
		noData: f.weights === null,
		active: f.id === input.familyId
	}));
	const family = harvest?.families.find((f) => f.id === input.familyId) ?? null;
	const view: PageView = { state: state.kind, stateLine: STATE_LINES[state.kind] ?? null, tabs, status: null, body: null };
	if (!family) return view;
	const status =
		(state.kind === 'ready' || state.kind === 'stale') && exchange
			? statusLine(state, exchange, family, input.horizon, input.divineChaosRate, input.lastFetchedAt, input.now)
			: null;
	if (!family.weights) return { ...view, status, body: noDataBody(family) };
	if (state.kind === 'warming') {
		return {
			...view,
			body: {
				kind: 'cold',
				title: 'No prices yet',
				text: 'The server has not read an hour of the exchange. This fills in on its own.'
			}
		};
	}
	if ((state.kind !== 'ready' && state.kind !== 'stale') || !exchange) return view;
	return { ...view, status, body: familyBody(input, family, exchange) };
}
