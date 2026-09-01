/**
 * Lab Dashboard API service — desktop edition.
 * Fetches real data from the Go backend using store.status.server_url.
 */

import { store } from '$lib/stores/status.svelte';
import type { TradeLookupResult } from './tradeApi';
import { getVersion } from '@tauri-apps/api/app';

// --- Types ---

export interface StatusData {
	lastUpdate: string;
	nextFetch: string;
	connected: boolean;
	collectorUptime: string;
	divinePrice: number;
}

export type PriceTier = 'TOP' | 'HIGH' | 'MID' | 'LOW' | '';
export type SellUrgency = 'SELL_NOW' | 'UNDERCUT' | 'HOLD' | 'WAIT' | '';
export type SellabilityLabel = 'FAST SELL' | 'GOOD' | 'MODERATE' | 'SLOW' | 'UNLIKELY' | '';

export interface GemPlay {
	name: string;
	baseName: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	/** 'OK' | 'LOW' | 'NO_BASE'. NO_BASE means the base gem is unpriced, so roi/basePrice are unknown — not zero. */
	confidence: string;
	roi: number;
	roiPercent: number;
	weightedRoi: number;
	signal: string;
	cv: number;
	windowSignal: string;
	advancedSignal: string;
	liquidityTier: string;
	transListings: number;
	transVelocity: number;
	baseListings: number;
	baseVelocity: number;
	basePrice: number;
	transPrice: number;
	sparkline: number[];
	sparklineListings: number[];
	signalHistory: SignalTransition[];
	priceTier: PriceTier;
	lowConfidence: boolean;
	tierAction: string;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
	low7d: number;
	high7d: number;
	histPosition: number;
	sellConfidence: string;
	tradeConfidenceNote: string;
	gcpRecipeCost: number;
	gcpRecipeBase: number;
	gcpRecipeSaves: number;
}

export interface SignalTransition {
	time: string;
	from: string;
	to: string;
	reason: string;
	listings: number;
}

export interface FontColor {
	color: 'RED' | 'GREEN' | 'BLUE';
	ev: number;
	pool: number;
	/** How many of `pool` carry a price at this variant. Below `pool` means missing prices, not a smaller pool. */
	pricedGems: number;
	winners: number;
	pWin: number;
	profit: number;
	evDelta2h: number;
	fontsToHit?: number;
	avgWin?: number;
	avgWinRaw?: number;
	evRaw?: number;
	jackpotGems?: { name: string; chaos: number; gcpRecipeCost: number; gcpRecipeBase: number; gcpRecipeSaves: number }[];
	thinPoolGems?: number;
	liquidityRisk?: string;
	mode?: string;
	poolBreakdown?: { tier: string; count: number; minPrice: number; maxPrice: number }[];
	lowConfidenceGems?: { name: string; chaos: number; listings: number }[];
}

export interface FontEVData {
	colors: FontColor[];
	qualityAvgRoi: number;
	bestColor: string;
	bestAdvantage: number;
}

export interface FontEVResponse {
	safe: FontColor[];
	premium: FontColor[];
	jackpot: FontColor[];
	bestColorSafe: string;
	bestColorPremium: string;
	bestColorJackpot: string;
}

// --- Dedication types ---

export interface DedicationColor {
	color: 'RED' | 'GREEN' | 'BLUE';
	/** Corrupted variant this row was computed over, e.g. "21/23c". */
	variant: string;
	gemType: string;
	pool: number;
	winners: number;
	pWin: number;
	avgWinRaw: number;
	evRaw: number;
	inputCost: number;
	profit: number;
	fontsToHit: number;
	jackpotGems?: { name: string; chaos: number }[];
	thinPoolGems: number;
	liquidityRisk: string;
	poolBreakdown?: { tier: string; count: number; minPrice: number; maxPrice: number }[];
	lowConfidenceGems?: { name: string; chaos: number; listings: number }[];
}

export interface DedicationPoolResponse {
	safe: DedicationColor[];
	premium: DedicationColor[];
	jackpot: DedicationColor[];
}

export interface DedicationEVResponse {
	skills: DedicationPoolResponse;
	transfigured: DedicationPoolResponse;
	/** Corrupted variant these pools were computed over, e.g. "21/23c". */
	variant: string;
	entryFee: number;
}

/**
 * The corrupted variants the Dedication views can be shown for, in display
 * format. Each is its own market: pool, tiers, input cost and EV are computed
 * per variant and never merged. Order is display order — 21/20 first, since
 * that is the cheaper input and the more common play.
 */
export const DEDICATION_VARIANTS = ['21/20', '21/23'] as const;
export type DedicationVariant = typeof DEDICATION_VARIANTS[number];

/**
 * The two Dedication gem pools, in the canon spelling — the one Rust, the
 * settings file and the database use. The Font EV JSON wire format spells the
 * first one `skills`; that boundary is translated in FontEVCompare rather than
 * renamed here, because the web frontend shares the wire key.
 */
export const DEDICATION_POOLS = ['skill', 'transfigured'] as const;
export type DedicationPool = typeof DEDICATION_POOLS[number];

/**
 * Display labels for the pools. Every surface that names a pool reads them from
 * here — they were previously written out at each call site, so renaming one
 * label meant finding six of them.
 *
 * "Non-Transfigured" rather than "Normal": the top bar already labels the lab
 * mode Normal, and two adjacent buttons both reading Normal would name two
 * different things.
 */
export const DEDICATION_POOL_LABELS: Record<DedicationPool, string> = {
	skill: 'Non-Transfigured',
	transfigured: 'Transfigured',
};

export interface MarketOverviewData {
	avgTransPrice: number;
	avgTransPriceDelta: number;
	avgBaseListings: number;
	avgBaseListingsDelta: number;
	activeGems: number;
	mostVolatileColor: string;
	mostVolatileCV: number;
	mostStableColor: string;
	mostStableCV: number;
	temporalMode: string;
	divineRate: number;
	sellConfidenceSpread: Record<string, number>;
	signalDistribution: Record<string, number>;
	offerings: {
		name: string;
		fragmentId: string;
		currentPrice: number;
		cheapHours: { hour: number; median: number }[];
		expensiveHours: { hour: number; median: number }[];
		cheapDays: { day: string; median: number }[];
		expensiveDays: { day: string; median: number }[];
		hourlyMedians: { hour: number; median: number }[];
		todayHourMedians: { hour: number; median: number }[];
		sparkline: { time: string; price: number }[];
	}[];
}

export interface CompareGem {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	/** 'OK' | 'LOW' | 'NO_BASE' | 'NO_DATA'. NO_DATA means the gem is unpriced — its zeros are unknown, not 0c. */
	confidence: string;
	roi: number;
	roiPercent: number;
	weightedRoi: number;
	signal: string;
	cv: number;
	transListings: number;
	transVelocity: number;
	baseListings: number;
	baseVelocity: number;
	basePrice: number;
	transPrice: number;
	liquidityTier: string;
	windowSignal: string;
	sparkline: number[];
	signalHistory: SignalTransition[];
	recommendation: 'BEST' | 'OK' | 'AVOID';
	priceTier: PriceTier;
	tierAction: string;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
	low7d: number;
	high7d: number;
	histPosition: number;
	sellConfidence: string;
	sellConfidenceReason: string;
	quickSellPrice: number;
	riskAdjustedPrice: number;
	/**
	 * Double-corruption at Doryani's Institute (POE-125), in chaos.
	 *
	 * `doubleCorruptModel` is the server's honesty marker ('estimated'): the odds
	 * behind these numbers come from community documentation, not from GGG, so no
	 * surface may render them as a settled price. Empty when the server did not
	 * model this gem at this variant — that is the flag to test before showing
	 * anything, since an unmodelled gem carries a 0 EV that means "unknown".
	 *
	 * `doubleCorruptEv` and `doubleCorruptProfit` share one basis — both are
	 * risk-adjusted. The profit is the EV minus the gem's own price at this
	 * variant: the claim "the corrupted market pays more than selling it here
	 * does". Two bases would let a card show a profit larger than the estimate
	 * it came from.
	 *
	 * `doubleCorruptPricedProbability` is the share of the outcome distribution
	 * that EV covers (0-1). The remainder is mass the corrupted market prices
	 * nowhere — largely structural, since poe.ninja publishes no corrupted
	 * variant below level 20 — so the EV is a floor, not a full expectation, and
	 * must not be shown without it.
	 *
	 * `doubleCorruptTiebreak` marks the one candidate whose BEST was decided by
	 * that profit rather than by the Font score — nothing in the comparison sold
	 * well, so the server promoted a gamble. It ships as a separate flag rather
	 * than a recommendation value precisely so the badge can be rendered
	 * differently from an ordinary win.
	 */
	doubleCorruptEv: number;
	doubleCorruptProfit: number;
	doubleCorruptModel: string;
	doubleCorruptTiebreak: boolean;
	doubleCorruptPricedProbability: number;
	trade?: TradeLookupResult;
}

// --- Currency Exchange (POE-175) ---

/**
 * Which plays `GET /currency-exchange/plays` returns: every ranked play, only
 * the single-swap ones, or only the two-swap ones. `'all'` is an explicit
 * filter value rather than the absence of one, so it is sent like any other.
 *
 * `$lib/exchange/view.ts` re-exports this type, which is what keeps the page's
 * mode picker and this fetcher from drifting apart.
 */
export type CurrencyExchangeMode = 'all' | 'direct' | '1-hop';

/**
 * Which window `GET /currency-exchange/plays` ranks in. `'recent'` is a short
 * window that reacts within hours and is what the server assumes when the
 * parameter is absent; `'day'` is the overnight-planning window, where a play
 * has to have held for most of a day to be listed at all.
 *
 * The server computes both windows in one recompute and answers a horizon it
 * does not know with a 400, so this is sent explicitly like `mode` is.
 */
export type CurrencyExchangeHorizon = 'recent' | 'day';

/**
 * One swap inside a play, as ONE feed hour observed it — the hour the play's
 * `lastHour` names, and no other. `item`/`quote` are the raw Currency Exchange
 * ids and `itemName`/`quoteName` their display names — real item names
 * (POE-177), with the humanized id as the fallback for an id the server's asset
 * does not know. `price` is quote per item.
 *
 * `price` is that hour's realized extreme on the leg's side (cheapest for a buy,
 * dearest for a sell) and it is RAW: the undercut a real order has to pay lives
 * in the play's `roiPct`/`roi`, never in this number. `fair` is the same hour's
 * volume-weighted price, where the traded mass actually cleared, and is the
 * anchor `price` should be read against; `fairOk` is false when the hour's quote
 * side reported no volume, and `fair` is then 0 — a missing reading rather than
 * a free item. `suspect` is that comparison already made: an extreme too far
 * from `fair` to be repeatable (a buy under `fair × 0.67`, a sell over
 * `fair × 1.5`), and it stays false when there is no `fair` to judge against.
 * `tick` is the hour's price resolution on this market as a fraction of the
 * price (0.5 = the next representable price is 50% away), which is what
 * separates a real spread from one integer step (POE-184) — and what lets a
 * client rebuild the undercut price: `price × (1 + tick)` to buy,
 * `price × (1 − tick)` to sell.
 *
 * `depletedSide` is a reading of the OPPOSITE side of this leg's book: true when
 * nothing stood there in the hour — no asks facing a sell, no bids facing a buy
 * — so the order this leg describes would have been alone on its side of the
 * market. It is the SHAPE of the book and not a verdict on the price: an empty
 * facing side is part of why the extreme was reachable at all, and it is equally
 * why an order posted into it may stand unmatched. It is a per-leg fact like
 * `suspect`, and the two are independent — a leg can be one-sided at a perfectly
 * ordinary price, and a leg far outside its fair band can have had a full book
 * on both sides.
 *
 * OPTIONAL on the wire — a server older than the field omits it, and an absent
 * field means false, so no client may distinguish "not sent" from "both sides
 * stood".
 *
 * `priceItemQty`/`priceQuoteQty` are that same price as the in-game Currency
 * Exchange posts it: the reduced integer quantity pair behind this hour's
 * extreme, with `priceQuoteQty / priceItemQty === price` EXACTLY — the server
 * builds the pair beside the price and never re-derives either from the other.
 * The game trades whole quantities on both sides, so a leg with `priceItemQty`
 * 4 and `priceQuoteQty` 1 quoted in divine is an order reading "sell 4 for 1
 * div"; the 0.25 divine it divides out to is not a price the exchange can
 * express, which is why the pair travels rather than being reconstructed here.
 * They are oriented to the LEG — `priceItemQty` counts units of `item` and
 * `priceQuoteQty` units of `quote`, whichever side of the raw feed row those
 * turn out to be — so a direct flip's two legs quote one market with two
 * different pairs, the buy leg carrying the hour's low and the sell leg its
 * high.
 *
 * It is the pair the extreme PRINTED at, and not the order to post: the order
 * that actually fills sits one quantity step tighter, is not served, and is
 * already paid for in `roiPct`/`roi`. Client copy must not promise that posting
 * this exact pair is the play.
 *
 * `itemIcon`/`quoteIcon` are API-RELATIVE paths into this server's icon route
 * (`/currency-exchange/icon/<escaped id>`), not upstream poewiki URLs —
 * production cannot reach poewiki (ADR-012). Join them onto `getApiBase()` with
 * `$lib/exchange/view`'s `iconSrc`. `null` means the item has no artwork, which
 * is a different thing from a path that later 404s: the first renders no icon
 * at all, the second the "?" fallback.
 *
 * `itemCategory`/`quoteCategory` are the leg's two sides placed in the in-game
 * Currency Exchange sidebar — one of the sixteen names the response's
 * `categories` lists, or `""` for an id the server's item asset does not cover.
 * `""` is a usable answer rather than an ambiguous one: a client filtering by
 * category treats an uncategorised side as UNFILTERED, so it never has to tell
 * an absent category from an empty one, and the fields are plain strings for
 * that reason. Both sides carry one because a filter applies to whichever side
 * the reader is shopping for.
 */
export interface CurrencyExchangeLeg {
	action: 'buy' | 'sell';
	item: string;
	quote: string;
	price: number;
	priceItemQty: number;
	priceQuoteQty: number;
	fair: number;
	fairOk: boolean;
	tick: number;
	volume: number;
	stock: number;
	suspect: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	depletedSide?: boolean;
	itemName: string;
	itemIcon: string | null;
	itemCategory: string;
	quoteName: string;
	quoteIcon: string | null;
	quoteCategory: string;
}

/**
 * A ranked arbitrage play: ONE hour's prices, plus the cross-hour readings of
 * how the recipe has actually behaved. `direct` swaps back through the same
 * market; `1-hop` routes through an intermediate currency.
 *
 * `expectedRoi` is the number the server RANKS on (POE-193, ADR-016): the mean
 * chaos one exchanged unit actually returned across the simulated entries of
 * the last day — post this play's undercut orders in each of those hours, chase
 * the buy, wait for the sell, fire-sale whatever never sold. It can be
 * NEGATIVE, and such a play is still served. The SERVED order is clean before
 * `suspect`, then covered before `lowCoverage`, then `expectedRoi` desc — a
 * chaos payout, so a bigger stake outranks a better rate.
 *
 * That order is the wire's, and the desktop does not display it: the table
 * re-sorts every response by the column the sort picker names and by nothing
 * else, flags included (`exchange/view.ts` `sortPlays`, POE-220). What survives
 * of the served order on screen is its tie-breaks, which a stable sort carries
 * through rows tied on the picked column.
 *
 * OPTIMISTIC is the word here on purpose: this block words the WIRE, and its
 * wording mirrors the Go docs on the fields it describes. The desktop-surface
 * BEST CASE / UNCONTESTED sweep (`docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §6.4,
 * which scopes itself to `tooltips.ts`, `view.ts` and the page caption)
 * deliberately stops here — align this with `internal/exchange/plays.go` if that
 * side is ever renamed, rather than with the row's reader-facing vocabulary.
 *
 * `roiPct` is the OPTIMISTIC reading: that one hour's round trip at the
 * UNDERCUT prices (0.123 = 12.3%), each leg paying one of its own ticks to be
 * the order that fills. Measured over 960 top-20 play-hours it overstates what
 * the play realized by 4-8x, which is why it no longer ranks anything — but it
 * is still what the client's quality gates judge, because those levels were
 * calibrated against it (ADR-015). `roiPctRaw` is the same round trip at the
 * raw extremes the legs show — never below `roiPct`, and the gap between the
 * two is what the ticks cost. `roi` is chaos gained per exchange of one unit
 * and `investment` the chaos that one unit costs to enter, both priced at the
 * undercut entry, so `roi === roiPct * investment` still holds by construction
 * for that optimistic pair. `expectedRoiPct` carries no such identity: it is a
 * mean of per-entry fractions, each with its own chased outlay, and NOT
 * `expectedRoi` divided by `investment`.
 *
 * `turnover` is chaos per hour through the play's thinnest leg, which is the
 * liquidity reading — `depth` (units per hour) is not one. `tick` is the
 * coarsest leg tick, the worst price step the recipe has to live with.
 * `suspect` is true when any leg is; such a play is still served — the reader
 * judges it — and the wire ranks it after every clean one, an ordering the
 * desktop table drops (see above). `hoursSeen` counts the window
 * hours in which the recipe cleared every gate on that hour's own prices, and
 * `lastHour` is the hour every price above was read from: always the window's
 * newest, because a recipe that did not clear in the last snapshot is not
 * served at all.
 *
 * `lowLiquidity` is the play-level thin-hour flag, and it is about `lastHour`
 * alone: that hour printed no spread worth taking, the round trip at its
 * undercut prices returning less than the server's floor. The PREDICATE is that
 * spread and the name is the cause the measured case showed — an hour so quiet
 * that the little which traded all cleared at one ratio — so `roiPct` on such a
 * play is a real measurement and may be NEGATIVE, down to the -1 a market whose
 * whole price step is 100% arithmetically returns.
 *
 * It is not a verdict on the market and it is not a ranking key. `expectedRoi`
 * reads the hours this flag lacks, so a recipe whose quiet hour is an exception
 * keeps its place and one that never had a spread sinks on its own — which is
 * why the flag marks the ROW, replaces no number on it, and is never a reason to
 * hide the play (it was a DROP until 2026-08-22, and what the drop removed was a
 * card flipping at 70-92% in five of the window's other six hours).
 *
 * OPTIONAL on the wire — a server older than the field omits it, and an absent
 * field means false, so no client may distinguish "not sent" from "clean".
 *
 * `simEntries` is how many entry hours the two expectations are averaged over
 * and `lowCoverage` says that count fell under the server's guard — "we could
 * not measure this", which is a different claim from "we measured this and it
 * loses" and ranks ahead of neither. Those two and the two expectations are
 * fields a client CANNOT recheck from the row it is shown in: their inputs are
 * hours the row does not carry. Neither can `turnover` or `hoursSeen`, for
 * their own reasons — the first is the hour's quote-side volume, which the legs
 * do not ship, and the second counts hours. What IS recheckable is the price
 * arithmetic: `roiPct`, `roi`, `investment`, `tick` and `depth` are a ratio, a
 * product, a min or a max over the legs on this row.
 */
export interface CurrencyExchangePlay {
	key: string;
	mode: 'direct' | '1-hop';
	legs: CurrencyExchangeLeg[];
	roiPct: number;
	/** @deprecated use `roiPct` — the server sends both with the same value. */
	edge: number;
	roiPctRaw: number;
	roi: number;
	investment: number;
	expectedRoi: number;
	expectedRoiPct: number;
	simEntries: number;
	lowCoverage: boolean;
	turnover: number;
	tick: number;
	depth: number;
	suspect: boolean;
	/** Optional: absent on a server older than the field, and absent means false. */
	lowLiquidity?: boolean;
	hoursSeen: number;
	lastHour: string;
}

/**
 * The `/currency-exchange/plays` payload. A cold server answers 200 with
 * `warm: false` and an empty `plays`, so an empty list is not an error — the
 * page tells the two apart through `warm`. `count` is the size after the mode
 * filter and `plays` arrives already ranked.
 *
 * `horizon` echoes the window these plays were ranked in, so a body arriving
 * from the cache cannot be mistaken for the other horizon's. `divineChaosRate`
 * is the newest hour's chaos value of one divine, and because a play is served
 * only if it cleared in that hour, it is the rate every divine-quoted `roi`,
 * `investment` and `turnover` here was valued with. It is 0 when that hour
 * carried no divine/chaos trade, in which case no divine-quoted play is in the
 * list at all.
 *
 * `categories` is the in-game Currency Exchange sidebar, in sidebar order: the
 * whole taxonomy, on every served body, INDEPENDENT of the plays in it — a cold
 * body with no plays still carries all sixteen. It is the source the category
 * filter renders from; never hold a copy of the list client-side, or a category
 * GGG adds becomes a silent gap in the filter while the legs already name it.
 */
export interface CurrencyExchangeResponse {
	league: string;
	lastUpdated: string | null;
	from: string | null;
	to: string | null;
	hours: number;
	warm: boolean;
	mode: CurrencyExchangeMode;
	horizon: CurrencyExchangeHorizon;
	divineChaosRate: number;
	count: number;
	plays: CurrencyExchangePlay[];
	categories: string[];
}

// --- API helpers ---

/** Cached app version — resolved once, reused on every request. */
let cachedVersion = '';
getVersion().then(v => { cachedVersion = v; }).catch(() => {});

/**
 * Build device identity headers for server requests.
 * Omits headers when values are not yet available (prevents sending "undefined").
 */
function deviceHeaders(): Record<string, string> {
	const headers: Record<string, string> = {};
	const deviceId = store.status?.device_id;
	if (deviceId && typeof deviceId === 'string') {
		headers['X-Device-ID'] = deviceId;
	}
	if (cachedVersion) {
		headers['X-App-Version'] = cachedVersion;
	}
	return headers;
}

/**
 * Returns the API base URL from the Rust backend settings.
 * Called fresh on every request so it picks up server_url changes (e.g. after settings edit).
 */
export function getApiBase(): string {
	return (store.status?.server_url || import.meta.env.VITE_SERVER_URL || 'https://profitofexile.localhost') + '/api';
}

async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
	const url = new URL(`${getApiBase()}${path}`);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (v) url.searchParams.set(k, v);
		}
	}
	const resp = await fetch(url.toString(), { headers: deviceHeaders() });
	if (!resp.ok) {
		throw new Error(`API ${path}: ${resp.status} ${resp.statusText}`);
	}
	return resp.json();
}

// --- Mapping helpers ---

/**
 * The four non-corrupted gem variants, in display format. Shared so the dashboard's
 * per-variant fetch and the By Variant tabs cannot drift apart. Dedication mode does
 * not use these — it pools by skill/transfigured at the selected corrupted
 * variant (DEDICATION_VARIANTS).
 */
export const VARIANTS = ['1/0', '1/20', '20/0', '20/20'] as const;

/**
 * Convert a backend variant ("1", "20") to the UI's display format ("1/0", "20/0").
 * The backend stores zero-quality variants without the quality suffix and normalizes
 * "/0" away on inbound query params, but never restores it on responses — so UI code
 * comparing against "1/0"/"20/0" would never match. Variants that already carry a
 * quality ("1/20", "21/23") pass through unchanged.
 */
export function displayVariant(v: string): string {
	return /^\d+$/.test(v) ? `${v}/0` : v;
}

/** Map a backend collective/trends row to frontend GemPlay. */
function mapCollectiveRow(r: any): GemPlay {
	return {
		name: r.transfiguredName || r.name || '',
		baseName: r.baseName || '',
		variant: displayVariant(r.variant || ''),
		color: r.gemColor || '',
		confidence: r.confidence || '',
		roi: Math.round(r.roi || 0),
		roiPercent: Math.round(r.roiPct || 0),
		weightedRoi: Math.round(r.weightedRoi || 0),
		signal: r.signal || '',
		cv: Math.round(r.cv || 0),
		windowSignal: r.windowSignal || '',
		advancedSignal: r.advancedSignal || '',
		liquidityTier: r.liquidityTier || '',
		transListings: r.transfiguredListings || r.currentListings || 0,
		transVelocity: Math.round(r.priceVelocity || 0),
		baseListings: r.baseListings || 0,
		baseVelocity: Math.round(r.listingVelocity || 0),
		basePrice: Math.round(r.basePrice || 0),
		transPrice: Math.round(r.transfiguredPrice || r.currentPrice || 0),
		sparkline: Array.isArray(r.sparkline) ? r.sparkline.map((p: any) => p.price ?? p) : [],
		sparklineListings: Array.isArray(r.sparkline) ? r.sparkline.map((p: any) => p.listings ?? 0) : [],
		signalHistory: [],
		priceTier: (r.priceTier || '') as PriceTier,
		lowConfidence: !!r.lowConfidence,
		tierAction: r.tierAction || '',
		sellUrgency: (r.sellUrgency || '') as SellUrgency,
		sellReason: r.sellReason || '',
		sellability: r.sellability || 0,
		sellabilityLabel: (r.sellabilityLabel || '') as SellabilityLabel,
		low7d: Math.round(r.low7d || 0),
		high7d: Math.round(r.high7d || 0),
		histPosition: Math.round(r.histPosition || 0),
		sellConfidence: r.sellConfidence || '',
		tradeConfidenceNote: r.tradeConfidenceNote || '',
		gcpRecipeCost: Math.round(r.gcpRecipeCost || 0),
		gcpRecipeBase: Math.round(r.gcpRecipeBase || 0),
		gcpRecipeSaves: Math.round(r.gcpRecipeSaves || 0),
	};
}

/**
 * Map a backend compare row to frontend CompareGem.
 *
 * Exported for its unit test: this is the only place the server's JSON field
 * names are spelled out, so a silent rename on either side shows up here or not
 * at all.
 */
export function mapCompareRow(r: any): CompareGem {
	const sparkline = Array.isArray(r.sparkline)
		? r.sparkline.map((p: any) => p.price ?? p)
		: [];
	return {
		name: r.transfiguredName || '',
		variant: displayVariant(r.variant || ''),
		color: r.gemColor || '',
		confidence: r.confidence || '',
		roi: Math.round(r.roi || 0),
		roiPercent: Math.round(r.roiPct || 0),
		weightedRoi: Math.round(r.weightedRoi || 0),
		signal: r.signal || '',
		cv: Math.round(r.cv || 0),
		transListings: r.transListings || r.transfiguredListings || 0,
		transVelocity: Math.round(r.priceVelocity || 0),
		baseListings: r.baseListings || 0,
		baseVelocity: Math.round(r.listingVelocity || 0),
		basePrice: Math.round(r.basePrice || 0),
		transPrice: Math.round(r.transfiguredPrice || 0),
		liquidityTier: r.liquidityTier || '',
		windowSignal: r.windowSignal || '',
		sparkline,
		signalHistory: [],
		recommendation: (r.recommendation || 'OK') as 'BEST' | 'OK' | 'AVOID',
		priceTier: (r.priceTier || '') as PriceTier,
		tierAction: r.tierAction || '',
		sellUrgency: (r.sellUrgency || '') as SellUrgency,
		sellReason: r.sellReason || '',
		sellability: r.sellability || 0,
		sellabilityLabel: (r.sellabilityLabel || '') as SellabilityLabel,
		low7d: Math.round(r.low7d || 0),
		high7d: Math.round(r.high7d || 0),
		histPosition: Math.round(r.histPosition || 0),
		sellConfidence: r.sellConfidence || '',
		sellConfidenceReason: r.sellConfidenceReason || '',
		quickSellPrice: Math.round(r.quickSellPrice || 0),
		riskAdjustedPrice: Math.round(r.riskAdjustedPrice || 0),
		// Not rounded, unlike the prices above: the server promotes a tiebreak
		// winner on profit > 0, so rounding a 0.4c profit down to 0 here would let
		// the card contradict the badge the same payload already carries.
		doubleCorruptEv: r.doubleCorruptEv || 0,
		doubleCorruptProfit: r.doubleCorruptProfit || 0,
		doubleCorruptModel: r.doubleCorruptModel || '',
		doubleCorruptTiebreak: r.doubleCorruptTiebreak === true,
		doubleCorruptPricedProbability: r.doubleCorruptPricedProbability || 0,
		trade: r.trade ?? undefined,
	};
}

// --- Public API functions ---

/**
 * What the server says THIS device is entitled to (POE-203).
 *
 * Goes through `get`, so it carries the same `X-Device-ID` header
 * `/api/device/identify` does — that header IS the question, and without it
 * the server can only answer for an anonymous device.
 *
 * The body is `{ role: string, channel: "stable" | "beta", features: string[] }`,
 * returned UNVALIDATED: `stores/entitlements.svelte.ts` narrows it, because it
 * is also the module that owns what an unreadable answer falls back to.
 */
export async function fetchDeviceMe(): Promise<unknown> {
	return get<unknown>('/device/me');
}

export async function fetchStatus(): Promise<StatusData> {
	try {
		const status = await get<any>('/analysis/status');
		const lastUpdated = status.lastUpdated || '';
		const nextFetch = status.nextFetch || '';
		return {
			lastUpdate: lastUpdated,
			nextFetch,
			connected: status.cached === true,
			collectorUptime: '',
			divinePrice: status.divinePrice || 0,
		};
	} catch (err) {
		console.warn('[Status] Failed to fetch status:', err);
		return {
			lastUpdate: '',
			nextFetch: '',
			connected: false,
			collectorUptime: '',
			divinePrice: 0,
		};
	}
}

export async function fetchBestPlays(
	variant?: string,
	budget?: number,
	sort?: 'roi' | 'roiPercent',
	limit?: number,
	search?: string,
	mode?: string,
): Promise<GemPlay[]> {
	const params: Record<string, string> = {};
	if (variant) params.variant = variant;
	if (budget) params.budget = String(budget);
	if (sort === 'roiPercent') params.sort = 'pct';
	if (limit) params.limit = String(limit);
	if (search) params.search = search;
	if (mode) params.mode = mode;

	const resp = await get<{ count: number; data: any[] }>('/analysis/collective', params);
	return (resp.data || []).map(mapCollectiveRow);
}

export async function fetchVariantPlays(variant: string): Promise<GemPlay[]> {
	return fetchBestPlays(variant);
}

function mapFontRows(rows: any[]): FontColor[] {
	return rows.map((r: any) => ({
		color: r.color || '',
		ev: Math.round(r.ev || 0),
		pool: r.pool || 0,
		pricedGems: r.pricedGems ?? (r.pool || 0),
		winners: r.winners || 0,
		pWin: Math.round((r.pWin || 0) * 10000) / 100,
		profit: Math.round(r.profit || 0),
		evDelta2h: 0,
		fontsToHit: r.fontsToHit || 0,
		avgWin: r.avgWin ? Math.round(r.avgWin) : 0,
		avgWinRaw: r.avgWinRaw ? Math.round(r.avgWinRaw) : 0,
		evRaw: r.evRaw ? Math.round(r.evRaw) : 0,
		jackpotGems: r.jackpotGems || [],
		thinPoolGems: r.thinPoolGems || 0,
		liquidityRisk: r.liquidityRisk || 'LOW',
		mode: r.mode || '',
		poolBreakdown: r.poolBreakdown || [],
		lowConfidenceGems: r.lowConfidenceGems || [],
	}));
}

export async function fetchFontEV(variant: string): Promise<FontEVResponse> {
	const params: Record<string, string> = {};
	if (variant) params.variant = variant;

	const resp = await get<{
		safe: any[];
		premium: any[];
		jackpot: any[];
		bestColorSafe: string;
		bestColorPremium: string;
		bestColorJackpot: string;
	}>('/analysis/font', params);

	return {
		safe: mapFontRows(resp.safe || []),
		premium: mapFontRows(resp.premium || []),
		jackpot: mapFontRows(resp.jackpot || []),
		bestColorSafe: resp.bestColorSafe || '',
		bestColorPremium: resp.bestColorPremium || '',
		bestColorJackpot: resp.bestColorJackpot || '',
	};
}

function mapDedicationRows(rows: any[]): DedicationColor[] {
	return rows.map((r: any) => ({
		color: r.color ?? '',
		variant: r.variant ?? '',
		gemType: r.gemType ?? '',
		pool: r.pool ?? 0,
		winners: r.winners ?? 0,
		pWin: Math.round((r.pWin ?? 0) * 10000) / 100,
		avgWinRaw: Math.round(r.avgWinRaw ?? 0),
		evRaw: Math.round(r.evRaw ?? 0),
		inputCost: Math.round(r.inputCost ?? 0),
		profit: Math.round(r.profit ?? 0),
		fontsToHit: r.fontsToHit ?? 0,
		jackpotGems: r.jackpotGems ?? [],
		thinPoolGems: r.thinPoolGems ?? 0,
		liquidityRisk: r.liquidityRisk ?? 'LOW',
		poolBreakdown: r.poolBreakdown ?? [],
		lowConfidenceGems: r.lowConfidenceGems ?? [],
	}));
}

function mapDedicationPool(pool: any): DedicationPoolResponse {
	return {
		safe: mapDedicationRows(pool?.safe || []),
		premium: mapDedicationRows(pool?.premium || []),
		jackpot: mapDedicationRows(pool?.jackpot || []),
	};
}

export async function fetchDedicationEV(variant?: string): Promise<DedicationEVResponse> {
	const resp = await get<{
		skills: any;
		transfigured: any;
		variant: string;
		entryFee: number;
	}>('/analysis/dedication', variant ? { variant } : undefined);

	return {
		skills: mapDedicationPool(resp.skills),
		transfigured: mapDedicationPool(resp.transfigured),
		variant: resp.variant || '',
		entryFee: Math.round(resp.entryFee || 0),
	};
}

export async function fetchMarketOverview(): Promise<MarketOverviewData> {
	const resp = await get<MarketOverviewData>('/analysis/market-overview');
	return {
		avgTransPrice: resp.avgTransPrice || 0,
		avgTransPriceDelta: resp.avgTransPriceDelta || 0,
		avgBaseListings: resp.avgBaseListings || 0,
		avgBaseListingsDelta: resp.avgBaseListingsDelta || 0,
		activeGems: resp.activeGems || 0,
		mostVolatileColor: resp.mostVolatileColor || '',
		mostVolatileCV: resp.mostVolatileCV || 0,
		mostStableColor: resp.mostStableColor || '',
		mostStableCV: resp.mostStableCV || 0,
		temporalMode: resp.temporalMode || 'none',
		divineRate: resp.divineRate || 0,
		sellConfidenceSpread: resp.sellConfidenceSpread || {},
		signalDistribution: resp.signalDistribution || {},
		offerings: (resp.offerings || []).map((o: any) => ({
			name: o.name || '',
			fragmentId: o.fragmentId || '',
			currentPrice: o.currentPrice || 0,
			cheapHours: o.cheapHours || [],
			expensiveHours: o.expensiveHours || [],
			cheapDays: o.cheapDays || [],
			expensiveDays: o.expensiveDays || [],
			hourlyMedians: o.hourlyMedians || [],
			todayHourMedians: o.todayHourMedians || [],
			sparkline: (o.sparkline || []).map((p: any) => ({ time: p.time, price: p.price })),
		})),
	};
}

export async function fetchGemNames(query: string, mode?: string): Promise<string[]> {
	if (query.length < 2) return [];
	if (mode === 'dedication') {
		// Fetch both pools (skills + transfigured) and merge — both font options available per run.
		const [skills, transfigured] = await Promise.all([
			get<{ names: string[] }>('/analysis/gems/names', { q: query, limit: '15', corrupted: 'true', transfigured: 'false' }),
			get<{ names: string[] }>('/analysis/gems/names', { q: query, limit: '15', corrupted: 'true', transfigured: 'true' }),
		]);
		const merged = [...new Set([...(skills.names || []), ...(transfigured.names || [])])];
		merged.sort();
		return merged.slice(0, 15);
	}
	const resp = await get<{ names: string[] }>('/analysis/gems/names', { q: query, limit: '15' });
	return resp.names || [];
}

export async function fetchCompare(gems: string[], variant: string, signal?: AbortSignal, mode?: string): Promise<CompareGem[]> {
	const url = new URL(`${getApiBase()}/analysis/compare`);
	url.searchParams.set('gems', gems.join(','));
	url.searchParams.set('variant', variant);
	if (mode === 'dedication') {
		url.searchParams.set('mode', 'dedication');
	}
	const resp = await fetch(url.toString(), { signal, headers: deviceHeaders() });
	if (!resp.ok) {
		throw new Error(`API /analysis/compare: ${resp.status} ${resp.statusText}`);
	}
	const body = await resp.json() as { count: number; data: any[] };
	const results = (body.data || []).map(mapCompareRow);

	// Enrich with signal history in parallel
	const settled = await Promise.allSettled(
		results.map(async (gem) => {
			gem.signalHistory = await fetchSignalHistory(gem.name, gem.variant);
		})
	);
	settled.forEach((result, i) => {
		if (result.status === 'rejected') {
			console.warn(`[Comparator] Signal history failed for ${results[i].name}:`, result.reason);
			results[i].signalHistory = [];
		}
	});

	return results;
}

// --- Signal History ---

/**
 * Label for one signal transition's timestamp, as the comparator and Best Plays
 * rows render it verbatim.
 *
 * A bare time-of-day is only honest for a transition that happened today. The
 * endpoint serves whatever sits in a gem's ring within the server's retention
 * window (internal/lab: signalHistorySeedMaxDays, 14 days), so a gem that
 * stopped trading last week answers with last week's transitions — and "03:12"
 * on an in-game overlay reads as "03:12 today". Anything older than today gets
 * its age instead: the exact minute of a three-day-old transition is not
 * actionable, its age is.
 *
 * Whole calendar days, not elapsed hours, so "1d ago" means yesterday — which is
 * how the label is read.
 */
export function signalTransitionLabel(iso: string, now: Date = new Date()): string {
	const at = new Date(iso);
	if (Number.isNaN(at.getTime())) return '';

	const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	// Rounded, not floored: a DST shift makes the span 23 or 25 hours.
	const days = Math.round((midnight(now) - midnight(at)) / 86_400_000);
	if (days > 0) return `${days}d ago`;

	return at.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

export async function fetchSignalHistory(
	name: string,
	variant: string,
	limit = 4
): Promise<SignalTransition[]> {
	const resp = await get<{ name: string; variant: string; count: number; history: any[] }>(
		'/analysis/history',
		{ name, variant, limit: String(limit) }
	);
	const snapshots = resp.history || [];
	if (snapshots.length < 2) return [];

	// History comes in DESC order — reverse for chronological
	snapshots.reverse();

	// Diff consecutive entries to find signal changes
	const transitions: SignalTransition[] = [];
	for (let i = 1; i < snapshots.length; i++) {
		const prev = snapshots[i - 1];
		const curr = snapshots[i];
		const time = signalTransitionLabel(curr.time || '');
		const reason = deriveReason(prev, curr);
		transitions.push({
			time,
			from: prev.signal || 'STABLE',
			to: curr.signal || 'STABLE',
			reason,
			listings: curr.currentListings || curr.listings || 0,
		});
	}
	return transitions;
}

function deriveReason(prev: any, curr: any): string {
	const priceDiff = (curr.currentPrice || 0) - (prev.currentPrice || 0);
	if (Math.abs(priceDiff) > 1) {
		return `${priceDiff > 0 ? '+' : ''}${Math.round(priceDiff)}c`;
	}
	const vel = curr.priceVelocity || 0;
	if (Math.abs(vel) > 1) {
		return `${vel > 0 ? '+' : ''}${Math.round(vel)}c/h`;
	}
	return 'velocity settled';
}


/**
 * Fetch the ranked Currency Exchange plays for one mode and horizon.
 *
 * `mode` is always sent, `'all'` included: the server reads an absent mode as
 * a default rather than as "no filter", and answers 400 to an unknown one.
 * `horizon` is sent explicitly for the same reason — the request states which
 * window it wants instead of inheriting whatever the server's default becomes,
 * and the response echoes it back. `'recent'` is the default here because it is
 * the server's; POE-186 gives the page a picker that passes the other one.
 *
 * Unlike `fetchStatus`, this throws on a failed request — the caller keeps the
 * result it already has and marks it stale instead of blanking the table.
 */
export async function fetchCurrencyExchangePlays(
	mode: CurrencyExchangeMode,
	horizon: CurrencyExchangeHorizon = 'recent'
): Promise<CurrencyExchangeResponse> {
	return get<CurrencyExchangeResponse>('/currency-exchange/plays', { mode, horizon });
}

// --- Mercure SSE ---

export interface MercureConnection {
	close: () => void;
	connected: boolean;
}

/**
 * The Mercure topic the server publishes on when a Currency Exchange hour
 * closes. Exported because the subscribe URL and the dispatch branch below
 * must name the same string, and because tests assert on it.
 */
export const CURRENCY_EXCHANGE_UPDATED_TOPIC = 'poe/currency-exchange/updated';

/**
 * Open the Mercure SSE stream and keep it open (token refresh, backoff,
 * visibility-aware reconnect). One connection serves the whole app — LabPage
 * owns it and fans the per-topic payloads out to the other pages.
 *
 * Dispatch is per topic, and each specific topic returns instead of falling
 * through: `poe/lab/layout` reaches `onLayoutUpdate` only, and
 * `poe/currency-exchange/updated` reaches `onCurrencyExchangeUpdate` only, so
 * a Currency Exchange tick does not drag the lab dashboard through a reload.
 * Everything else — `poe/analysis/updated` above all — calls `onUpdate`.
 *
 * @param onUpdate Fired for every topic without a dedicated callback.
 * @param onConnectionChange Connection state, debounced by 5s on the way down.
 * @param onLayoutUpdate `poe/lab/layout` payloads.
 * @param onCurrencyExchangeUpdate `poe/currency-exchange/updated` payloads.
 *   Optional like the rest: three-argument callers keep working unchanged and
 *   simply drop those events.
 */
export function connectMercure(onUpdate: () => void, onConnectionChange?: (connected: boolean) => void, onLayoutUpdate?: (data?: any) => void, onCurrencyExchangeUpdate?: (data?: any) => void): MercureConnection {
	const state: MercureConnection = { close: () => {}, connected: false };
	let eventSource: EventSource | null = null;
	let tokenTimeout: ReturnType<typeof setTimeout> | null = null;
	let retries = 0;
	let disconnectTimer: ReturnType<typeof setTimeout> | null = null;
	let visibilityHandler: (() => void) | null = null;
	// Set by close(). Every async continuation re-checks it before touching the
	// connection — see the guard after the token fetch in connect(), and
	// frontend/src/lib/desktopBridge.ts, which carries the same flag.
	let closed = false;

	// Attempts spent in the fast lane before the delay escalates. 2s/4s/8s/10s/10s
	// recovers quickly from a deploy; a longer outage should not keep costing a
	// token fetch every 10s forever (360 req/h per client, POE-151).
	const FAST_RETRIES = 5;
	const SLOW_RETRY_MS = 60000;

	function retryDelay(): number {
		// Exponential backoff: 2s, 4s, 8s, capped at 10s (fast recovery after
		// deploys), then a 60s slow lane. desktopBridge.ts doubles straight to the
		// same 60s constant with no fast lane and no jitter, so the two share that
		// number and nothing else about the ladder — and the jitter below means
		// this one's slow lane is really 30-60s.
		const base = retries < FAST_RETRIES
			? Math.min(2000 * Math.pow(2, retries), 10000)
			: SLOW_RETRY_MS;
		// Equal jitter: half the delay is fixed, half is random. A deploy restarts
		// every client at once, and an un-jittered backoff puts all of them back on
		// the same grid — the retry then arrives as one wave instead of a spread.
		// Full jitter (random(0, base)) would spread it too, but also lets a client
		// retry ~instantly, which is the case we are trying to bound.
		return base / 2 + Math.random() * (base / 2);
	}

	/**
	 * Arm the next reconnect. Increments the attempt counter and logs the delay it
	 * actually sleeps — the previous code logged the pre-increment delay, so
	 * "retrying in 4s" was followed by an 8s wait.
	 */
	function scheduleReconnect(reason: string, err?: unknown) {
		if (closed) return;
		const delay = retryDelay();
		retries++;
		const message = `[Mercure] ${reason}, retrying in ${Math.round(delay / 1000)}s (attempt ${retries})`;
		if (err === undefined) console.warn(message);
		else console.warn(message, err);
		if (tokenTimeout) clearTimeout(tokenTimeout);
		tokenTimeout = setTimeout(connect, delay);
	}

	function closeEventSource() {
		if (eventSource) {
			eventSource.onopen = null;
			eventSource.onmessage = null;
			eventSource.onerror = null;
			eventSource.close();
			eventSource = null;
		}
	}

	async function connect() {
		if (closed) return;
		try {
			const tokenResp = await get<{ token: string; url: string }>('/mercure/token');
			const { token, url } = tokenResp;
			// close() can land inside the token-fetch RTT. Without this guard the
			// EventSource below is created after the caller has already dropped its
			// reference to this connection: nobody can ever close it, it re-arms its
			// own 25-minute refresh, and the hub holds the dead subscriber until its
			// write_timeout (480-600s) expires. POE-151.
			if (closed) return;

			closeEventSource();

			const authedUrl = new URL(url);
			authedUrl.searchParams.append('topic', 'poe/analysis/updated');
			authedUrl.searchParams.append('topic', 'poe/lab/layout');
			authedUrl.searchParams.append('topic', CURRENCY_EXCHANGE_UPDATED_TOPIC);
			authedUrl.searchParams.set('authorization', token);

			eventSource = new EventSource(authedUrl.toString());

			eventSource.onopen = () => {
				state.connected = true;
				if (disconnectTimer) {
					clearTimeout(disconnectTimer);
					disconnectTimer = null;
				}
				onConnectionChange?.(true);
				// The only place the attempt counter resets. It used to also reset on a
				// successful token fetch, which pinned a stream that opens and then fails
				// at a permanent 4s retry — the backoff never escalated.
				retries = 0;
			};

			eventSource.onmessage = (event) => {
				let data: any;
				try {
					data = JSON.parse(event.data);
				} catch (err) {
					// Every legitimate publish is a marshalled JSON object
					// (internal/lab/throttler.go marshals a map[string]string), so
					// unparseable data is not an update — treating it as one fired the
					// full loadAll() fan-out and threw the parse error away. Matches
					// frontend/src/lib/api.ts.
					console.warn('[Mercure] Failed to parse message:', err, 'raw:', event.data);
					return;
				}

				if (data?.topic === 'poe/lab/layout') {
					onLayoutUpdate?.(data);
					return;
				}

				if (data?.topic === CURRENCY_EXCHANGE_UPDATED_TOPIC) {
					onCurrencyExchangeUpdate?.(data);
					return;
				}

				onUpdate();
			};

			eventSource.onerror = () => {
				closeEventSource();
				if (closed) return;
				// Debounce the disconnected indicator: only flip after 5s of sustained outage,
				// and only arm the timer on the first error in a retry sequence (not every retry).
				// state.connected flips inside the timer so both consumers (status store + callback
				// subscribers) observe the same single source of truth — no 5s window of contradiction.
				if (disconnectTimer === null) {
					disconnectTimer = setTimeout(() => {
						state.connected = false;
						onConnectionChange?.(false);
					}, 5000);
				}
				if (typeof document !== 'undefined' && document.hidden === true) {
					// Tab/window is hidden — don't burn retries while backgrounded.
					// Reconnect when visibility returns instead.
					console.warn('[Mercure] tab hidden, deferring reconnect until visible');
					// Cancel any pending periodic token refresh so it can't race the
					// visibility-driven reconnect when the tab returns.
					if (tokenTimeout) {
						clearTimeout(tokenTimeout);
						tokenTimeout = null;
					}
					// Replace any prior visibility handler before registering a new one,
					// otherwise stale listeners can leak across retry cycles.
					if (visibilityHandler && typeof document !== 'undefined') {
						document.removeEventListener('visibilitychange', visibilityHandler);
						visibilityHandler = null;
					}
					visibilityHandler = () => {
						visibilityHandler = null;
						connect();
					};
					document.addEventListener('visibilitychange', visibilityHandler, { once: true });
					return;
				}
				scheduleReconnect('SSE connection lost');
			};

			// Token TTL is 30min — refresh before expiry
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, 25 * 60 * 1000);
		} catch (err) {
			if (closed) return;
			// Single source of truth: state.connected flips inside the disconnect timer
			// so it stays consistent with the debounced onConnectionChange callback.
			if (disconnectTimer === null) {
				disconnectTimer = setTimeout(() => {
					state.connected = false;
					onConnectionChange?.(false);
				}, 5000);
			}
			scheduleReconnect('Connection failed', err);
		}
	}

	state.close = () => {
		// Set before anything else: an in-flight connect() reads this the moment its
		// token fetch resolves, and that is the only thing standing between a close()
		// during the RTT and an EventSource nobody holds a reference to.
		closed = true;
		closeEventSource();
		if (tokenTimeout) {
			clearTimeout(tokenTimeout);
			tokenTimeout = null;
		}
		if (disconnectTimer) {
			clearTimeout(disconnectTimer);
			disconnectTimer = null;
		}
		// Mirror the frontend variant's typeof guard for parity — desktop webview always
		// has document, but keeping the shape identical prevents drift between the two clients.
		if (visibilityHandler && typeof document !== 'undefined') {
			document.removeEventListener('visibilitychange', visibilityHandler);
			visibilityHandler = null;
		}
		state.connected = false;
		onConnectionChange?.(false);
	};

	connect();
	return state;
}
