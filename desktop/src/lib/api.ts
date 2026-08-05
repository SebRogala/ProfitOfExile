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
	trade?: TradeLookupResult;
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

/** Map a backend compare row to frontend CompareGem. */
function mapCompareRow(r: any): CompareGem {
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
		trade: r.trade ?? undefined,
	};
}

// --- Public API functions ---

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
		const time = new Date(curr.time || '').toLocaleTimeString(undefined, {
			hour: '2-digit',
			minute: '2-digit',
		});
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


// --- Mercure SSE ---

export interface MercureConnection {
	close: () => void;
	connected: boolean;
}

export function connectMercure(onUpdate: () => void, onConnectionChange?: (connected: boolean) => void, onLayoutUpdate?: (data?: any) => void): MercureConnection {
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
		// deploys), then a 60s slow lane — the same ceiling desktopBridge.ts uses.
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
				} catch {
					onUpdate();
					return;
				}

				if (data?.topic === 'poe/lab/layout') {
					onLayoutUpdate?.(data);
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
