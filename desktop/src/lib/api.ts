/**
 * Lab Dashboard API service — desktop edition.
 * Fetches real data from the Go backend at the saved server, which Rust carries
 * on the SSOT snapshot (`$lib/stores/server-base.svelte`, POE-383).
 */

import { store } from '$lib/stores/status.svelte';
import { getServerUrl, whenServerUrl } from '$lib/stores/server-base.svelte';
import type { TradeLookupResult } from './tradeApi';
import type { ExchangePriceRead, HarvestFamilyData } from '$lib/harvest/seam';
import { getVersion } from '@tauri-apps/api/app';
import type { CurrencyExchangeMode, CurrencyExchangeHorizon, ExchangePageParams, CurrencyExchangeResponse } from './exchange-types';

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
	ev: number | null;
	pool: number;
	/** Pool gems with solid price evidence at this variant. */
	pricedGems: number;
	coverage: number;
	topEvaluated?: boolean | null;
	winners: number;
	pWin: number;
	profit: number | null;
	evDelta2h: number;
	fontsToHit?: number;
	avgWin?: number;
	avgWinRaw?: number;
	evRaw: number | null;
	jackpotGems?: { name: string; chaos: number; gcpRecipeCost: number; gcpRecipeBase: number; gcpRecipeSaves: number }[];
	thinPoolGems?: number;
	liquidityRisk?: string;
	mode?: string;
	poolBreakdown?: { tier: string; count: number; minPrice: number; maxPrice: number }[];
	lowConfidenceGems?: { name: string; chaos: number; listings: number }[];
}

export interface FontEVResponse {
	safe: FontColor[];
	premium: FontColor[];
	jackpot: FontColor[];
	best: { variant: string; color: string } | null;
	/** Facetor's Lens craft quotes; null when the server predates the field. */
	lens: FontLensCraft | null;
}

/** One trade-queue quote: `chaos` null when unusable, `fetchedAt` null when never checked. */
export interface LensQuote {
	chaos: number | null;
	listings: number;
	fetchedAt: string | null;
	lowConfidence: boolean;
}

/** The Facetor's Lens 60% craft: lens (outcome), support (feeder) and the server's signed EV. */
export interface FontLensCraft {
	outcome: LensQuote & { tradeUrl?: string | null };
	feeder: LensQuote & { tradeUrl: string | null };
	ev: number | null;
	feeders?: {
		source: 'trade' | 'poe.ninja';
		name: string;
		chaos: number | null;
		fetchedAt: string | null;
		tradeUrl: string | null;
	}[];
	cheapest?: number | null;
	evCheapest?: number | null;
}

function mapLensQuote(q: any): LensQuote {
	return {
		chaos: typeof q?.chaos === 'number' ? q.chaos : null,
		listings: q?.listings ?? 0,
		fetchedAt: q?.fetchedAt ?? null,
		lowConfidence: q?.lowConfidence === true,
	};
}

export function mapFontLensCraft(lens: any): FontLensCraft | null {
	if (lens == null) return null;
	return {
		outcome: {
			...mapLensQuote(lens.outcome),
			...(lens.outcome?.tradeUrl !== undefined ? { tradeUrl: lens.outcome.tradeUrl } : {}),
		},
		feeder: { ...mapLensQuote(lens.feeder), tradeUrl: lens.feeder?.tradeUrl || null },
		ev: typeof lens.ev === 'number' ? lens.ev : null,
		...(Array.isArray(lens.feeders) ? {
			feeders: lens.feeders.map((feeder: any) => ({
				source: feeder.source,
				name: feeder.name,
				chaos: typeof feeder.chaos === 'number' ? feeder.chaos : null,
				fetchedAt: feeder.fetchedAt ?? null,
				tradeUrl: feeder.tradeUrl ?? null,
			})),
			cheapest: typeof lens.cheapest === 'number' ? lens.cheapest : null,
			evCheapest: typeof lens.evCheapest === 'number' ? lens.evCheapest : null,
		} : {}),
	};
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
	 * `doubleCorruptModel` is the server's honesty marker
	 * (`'estimated-provisional-destroy'` on current results — the destroy chance
	 * is a guess, too; `'estimated'` on rows computed before POE-378): the odds
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

export type { CurrencyExchangeMode, CurrencyExchangeHorizon, CurrencyExchangeLeg, CurrencyExchangePlay, PlayDisplay, ExchangeUniverseItem, ExchangePageParams, CurrencyExchangeResponse } from './exchange-types';

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
 * The API base URL (`<saved server>/api`), or null before the first SSOT
 * snapshot has carried the server. Read fresh on every call so a Settings edit
 * applies on the next request.
 */
export function getApiBase(): string | null {
	const server = getServerUrl();
	return server === null ? null : `${server}/api`;
}

async function get<T>(path: string, params?: Record<string, string | string[]>): Promise<T> {
	const url = new URL(`${await whenServerUrl()}/api${path}`);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (Array.isArray(v)) for (const each of v) url.searchParams.append(k, each);
			else if (v) url.searchParams.set(k, v);
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

export type RankingsSort = 'price' | 'roi' | 'riskAdjusted' | 'roiPercent';

/** The rankings controls, as one paged `/analysis/collective` request. */
export interface RankingsQuery {
	mode: 'normal' | 'dedication';
	/** Normal: 'ALL' or a variant. Dedication: the tab's market. */
	tab: string;
	/** Dedication tab pool; Normal leaves it unset. */
	pool?: DedicationPool;
	color: string;
	budget: number;
	pageSize: number;
	sort: RankingsSort;
	showLowConf: boolean;
	search?: string;
}

const RANKINGS_SORT_PARAM: Record<RankingsSort, string> = {
	price: 'price',
	roi: 'roi',
	riskAdjusted: 'weightedRoi',
	roiPercent: 'roiPct',
};

/**
 * Request params for one rankings page. A search names its gem explicitly, so
 * it carries no colour, budget, low-confidence or pool filter, and the server
 * ignores its pageSize (which still selects paged mode).
 */
export function collectiveParams(q: RankingsQuery): Record<string, string> {
	const params: Record<string, string> = {};
	if (q.mode === 'dedication') {
		params.mode = 'dedication';
		params.variant = q.tab;
	}
	if (q.search) {
		params.search = q.search;
	} else {
		if (q.mode === 'normal' && q.tab !== 'ALL') params.variant = q.tab;
		if (q.mode === 'dedication' && q.pool) params.pool = q.pool;
		if (q.color && q.color !== 'ALL') params.color = q.color;
		if (q.budget > 0) params.budget = String(q.budget);
		if (!q.showLowConf) params.hideLowConfidence = 'true';
	}
	params.pageSize = String(q.pageSize);
	params.sort = RANKINGS_SORT_PARAM[q.sort];
	return params;
}

export async function fetchRankingsPage(q: RankingsQuery): Promise<{ rows: GemPlay[]; total: number }> {
	const resp = await get<{ total?: number; data: any[] }>('/analysis/collective', collectiveParams(q));
	const rows = (resp.data || []).map(mapCollectiveRow);
	return { rows, total: resp.total ?? rows.length };
}

export async function fetchVariantPlays(variant: string): Promise<GemPlay[]> {
	return fetchBestPlays(variant);
}

function mapFontRows(rows: any[]): FontColor[] {
	return rows.map((r: any) => ({
		color: r.color || '',
		ev: r.ev == null ? null : Math.round(r.ev),
		pool: r.pool || 0,
		pricedGems: r.pricedGems ?? 0,
		coverage: r.coverage ?? 0,
		topEvaluated: r.topEvaluated,
		winners: r.winners || 0,
		pWin: Math.round((r.pWin || 0) * 10000) / 100,
		profit: r.profit == null ? null : Math.round(r.profit),
		evDelta2h: 0,
		fontsToHit: r.fontsToHit || 0,
		avgWin: r.avgWin ? Math.round(r.avgWin) : 0,
		avgWinRaw: r.avgWinRaw ? Math.round(r.avgWinRaw) : 0,
		evRaw: r.evRaw == null ? null : Math.round(r.evRaw),
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
		best?: { variant: string; color: string } | null;
		lens?: unknown;
	}>('/analysis/font', params);

	return {
		safe: mapFontRows(resp.safe || []),
		premium: mapFontRows(resp.premium || []),
		jackpot: mapFontRows(resp.jackpot || []),
		best: resp.best ? { variant: displayVariant(resp.best.variant), color: resp.best.color } : null,
		lens: mapFontLensCraft(resp.lens),
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
	const url = new URL(`${await whenServerUrl(signal)}/api/analysis/compare`);
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

	await enrichSignalHistory(results, signal);
	return results;
}

/**
 * Most keys one /analysis/history/bulk request carries (the server's
 * signalHistoryBulkMaxKeys); a larger compare is split into batches.
 */
const SIGNAL_HISTORY_BULK_MAX_KEYS = 12;

/**
 * Fills each card's signal history with one bulk request per batch of cards
 * (POE-410). A failed batch leaves its cards with no history and keeps the
 * compare rows; an abort rejects with the signal's reason and assigns nothing.
 */
async function enrichSignalHistory(results: CompareGem[], signal?: AbortSignal): Promise<void> {
	const batches: CompareGem[][] = [];
	for (let i = 0; i < results.length; i += SIGNAL_HISTORY_BULK_MAX_KEYS) {
		batches.push(results.slice(i, i + SIGNAL_HISTORY_BULK_MAX_KEYS));
	}
	await Promise.all(
		batches.map(async (batch) => {
			let histories: SignalTransition[][];
			try {
				histories = await fetchSignalHistoryBulk(batch, signal);
			} catch (err) {
				if (signal?.aborted) return;
				console.warn(`[Comparator] Signal history failed for ${batch.map((g) => g.name).join(', ')}:`, err);
				histories = batch.map(() => []);
			}
			if (signal?.aborted) return;
			batch.forEach((gem, i) => {
				gem.signalHistory = histories[i];
			});
		})
	);
	signal?.throwIfAborted();
}

async function fetchSignalHistoryBulk(gems: CompareGem[], signal?: AbortSignal, limit = 4): Promise<SignalTransition[][]> {
	const url = new URL(`${await whenServerUrl(signal)}/api/analysis/history/bulk`);
	for (const gem of gems) url.searchParams.append('key', `${gem.name}|${gem.variant}`);
	url.searchParams.set('limit', String(limit));
	const resp = await fetch(url.toString(), { signal, headers: deviceHeaders() });
	if (!resp.ok) {
		throw new Error(`API /analysis/history/bulk: ${resp.status} ${resp.statusText}`);
	}
	const body = (await resp.json()) as { data?: { history?: any[]; error?: string }[] };
	return gems.map((gem, i) => bulkEntryTransitions(gem, body.data?.[i]));
}

/**
 * One bulk entry's transitions; an error entry or a failed transform is [], and
 * an entry without history is [] as the single route's empty body is.
 */
function bulkEntryTransitions(gem: CompareGem, entry: { history?: any[]; error?: string } | undefined): SignalTransition[] {
	if (entry?.error) {
		console.warn(`[Comparator] Signal history failed for ${gem.name}:`, entry.error);
		return [];
	}
	try {
		return signalTransitions(entry?.history || []);
	} catch (err) {
		console.warn(`[Comparator] Signal history failed for ${gem.name}:`, err);
		return [];
	}
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
	return signalTransitions(resp.history || []);
}

/** Diffs a gem's history (newest first, as served) into its signal transitions. */
function signalTransitions(snapshots: any[]): SignalTransition[] {
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
 *
 * `page` is the paged request (ADR-030, built by `exchangePageParams`): its
 * `pageSize` selects the server's paged mode, which filters, sorts, counts and
 * caps in SQL. A repeated param (the rule lists) goes out once per value.
 */
export async function fetchCurrencyExchangePlays(
	mode: CurrencyExchangeMode,
	horizon: CurrencyExchangeHorizon,
	page: ExchangePageParams
): Promise<CurrencyExchangeResponse> {
	return get<CurrencyExchangeResponse>('/currency-exchange/plays', { ...page, mode, horizon });
}

/** One Harvest snapshot: prices and family tiers read together (POE-286 D10). */
export interface HarvestResponse {
	prices: ExchangePriceRead;
	families: HarvestFamilyData;
}

/**
 * The Harvest Flipping read for one horizon: exchange prices and the family
 * tiers built on them, from one server snapshot so the two never straddle a
 * recompute. Cold (`prices.warm: false`) answers with empty maps, not an error.
 *
 * Like `fetchCurrencyExchangePlays`, this throws on a failed request — the
 * caller keeps the result it already has.
 */
export async function fetchHarvest(horizon: CurrencyExchangeHorizon): Promise<HarvestResponse> {
	return get<HarvestResponse>('/harvest', { horizon });
}

/**
 * The divine:chaos rate every chaos ↔ divine figure converts with (POE-284),
 * owned by the Currency Exchange module (`internal/exchange`) and readable by
 * every device. `divineChaosRate` is a finite number > 0 or `null` (no exchange
 * hour yet, or no divine/chaos trade in it). `buyChaos` / `sellChaos` are the
 * exchange hour's cheapest / dearest divine trade in chaos, for information
 * only; nothing converts with them. `source` names where the rate came from:
 * the Currency Exchange, or the poe.ninja snapshot the server falls back to
 * while the exchange is cold. These are the source fields; the store
 * additionally derives stale and `unreachableSince` on failed reads.
 * Read through `$lib/stores/divine-rate.svelte`, never directly.
 */
export interface DivineRate {
	divineChaosRate: number | null;
	buyChaos: number | null;
	sellChaos: number | null;
	source: 'exchange' | 'ninja' | null;
	priceHour: string | null;
	updatedAt: string;
	state: 'ready' | 'cold' | 'stale';
	unreachableSince: string | null;
}

/**
 * Read the divine rate through its port: the Currency Exchange's own rate
 * (`GET /currency-exchange/divine-rate`). `state` is `ready` when the server
 * holds a rate and is warm, or answers from its poe.ninja fallback
 * (`source: 'ninja'`, sent while the exchange is cold); otherwise `cold`.
 * Released builds without this rule read a ninja fallback as cold. An older
 * server sends no `source` and only ever an exchange rate. Stale and
 * unreachable detection belongs to the store: when this rejects it keeps the
 * last value and marks it stale (POE-291).
 */
export async function fetchDivineRate(): Promise<DivineRate> {
	const body = await get<{
		divineChaosRate: number | null;
		buyChaos?: number | null;
		sellChaos?: number | null;
		source?: 'exchange' | 'ninja' | null;
		priceHour: string | null;
		warm: boolean;
	}>('/currency-exchange/divine-rate');
	return {
		divineChaosRate: body.divineChaosRate,
		buyChaos: body.buyChaos ?? null,
		sellChaos: body.sellChaos ?? null,
		source: body.source ?? (body.divineChaosRate !== null ? 'exchange' : null),
		priceHour: body.priceHour,
		updatedAt: new Date().toISOString(),
		state: body.divineChaosRate !== null && (body.warm || body.source === 'ninja') ? 'ready' : 'cold',
		unreachableSince: null
	};
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

/** Harvest snapshots are stored before the server publishes this topic. */
export const HARVEST_UPDATED_TOPIC = 'poe/harvest/updated';

export const LEAGUE_ACTIVE_TOPIC = 'poe/league/active';

/** Callbacks for {@link connectMercure}; see its doc for the routing. */
export interface MercureHandlers {
	/** Every topic without a dedicated handler. */
	onUpdate: () => void;
	/** Every stream open. */
	onOpen?: () => void;
	/** Connection state, debounced by 5s on the way down. */
	onConnectionChange?: (connected: boolean) => void;
	onLayoutUpdate?: (data: any) => void;
	onCurrencyExchangeUpdate?: (data: any) => void;
	onHarvestUpdate?: (data: any) => void;
	onLeagueActive?: (data: any) => void;
}

/**
 * Open the Mercure SSE stream and keep it open (token refresh, backoff,
 * visibility-aware reconnect). One connection serves the whole app — the
 * mercure store (`stores/mercure.svelte.ts`) owns it and fans the per-topic
 * payloads out to the other pages.
 *
 * Dispatch is per topic, and each specific topic returns instead of falling
 * through: `poe/lab/layout` reaches `onLayoutUpdate` only, and
 * `poe/currency-exchange/updated` reaches `onCurrencyExchangeUpdate` only, so
 * a Currency Exchange tick does not drag the lab dashboard through a reload.
 * `poe/harvest/updated` likewise reaches `onHarvestUpdate` only.
 * Everything else — `poe/analysis/updated` above all — calls `onUpdate`.
 *
 * `handlers.onOpen` fires on every stream open, after `onConnectionChange(true)`:
 * the server publishes `poe/league/active` once at boot, which a client still
 * reconnecting to the restarted server misses, so an open is itself a league
 * hint whose league is unknown. `onLeagueActive` receives only that topic's
 * payloads. Handlers left out drop their topic's events.
 */
export function connectMercure(handlers: MercureHandlers): MercureConnection {
	const { onUpdate, onOpen, onConnectionChange, onLayoutUpdate, onCurrencyExchangeUpdate, onHarvestUpdate, onLeagueActive } = handlers;
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
			authedUrl.searchParams.append('topic', HARVEST_UPDATED_TOPIC);
			authedUrl.searchParams.append('topic', LEAGUE_ACTIVE_TOPIC);
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
				onOpen?.();
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

				if (data?.topic === LEAGUE_ACTIVE_TOPIC) {
					onLeagueActive?.(data);
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

				if (data?.topic === HARVEST_UPDATED_TOPIC) {
					onHarvestUpdate?.(data);
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
