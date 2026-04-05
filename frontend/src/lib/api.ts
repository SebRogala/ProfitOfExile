/**
 * Lab Dashboard API service.
 * Fetches real data from the Go backend at /api/*.
 */

import { dispatchTradeEvent } from './tradeApi';

// --- Types ---

export interface StatusData {
	lastUpdate: string;
	nextFetch: string;
	connected: boolean;
	collectorUptime: string;
	divinePrice: number;
	league: string;
}

export type PriceTier = 'TOP' | 'HIGH' | 'MID' | 'LOW' | '';
export type SellUrgency = 'SELL_NOW' | 'UNDERCUT' | 'HOLD' | 'WAIT' | '';
export type SellabilityLabel = 'FAST SELL' | 'GOOD' | 'MODERATE' | 'SLOW' | 'UNLIKELY' | '';

export interface GemPlay {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
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
	winners: number;
	pWin: number;
	profit: number;
	evDelta2h: number;
	fontsToHit?: number;
	avgWin?: number;
	avgWinRaw?: number;
	evRaw?: number;
	jackpotGems?: { name: string; chaos: number }[];
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
}

// --- API helpers ---

const API_BASE = '/api';

async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
	const url = new URL(`${API_BASE}${path}`, window.location.origin);
	if (params) {
		for (const [k, v] of Object.entries(params)) {
			if (v) url.searchParams.set(k, v);
		}
	}
	const resp = await fetch(url.toString());
	if (!resp.ok) {
		throw new Error(`API ${path}: ${resp.status} ${resp.statusText}`);
	}
	return resp.json();
}

// --- Mapping helpers ---

/** Map a backend collective/trends row to frontend GemPlay. */
function mapCollectiveRow(r: any): GemPlay {
	return {
		name: r.transfiguredName || r.name || '',
		variant: r.variant || '',
		color: r.gemColor || '',
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
		variant: r.variant || '',
		color: r.gemColor || '',
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
			league: status.league || '',
		};
	} catch (err) {
		console.warn('[Status] Failed to fetch status:', err);
		return {
			lastUpdate: '',
			nextFetch: '',
			connected: false,
			collectorUptime: '',
			divinePrice: 0,
			league: '',
		};
	}
}

export async function fetchBestPlays(
	variant?: string,
	budget?: number,
	sort?: 'roi' | 'roiPercent',
	limit?: number,
): Promise<GemPlay[]> {
	const params: Record<string, string> = {};
	if (variant) params.variant = variant;
	if (budget) params.budget = String(budget);
	if (sort === 'roiPercent') params.sort = 'pct';
	if (limit) params.limit = String(limit);

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

export async function fetchGemNames(query: string): Promise<string[]> {
	if (query.length < 2) return [];
	const resp = await get<{ names: string[] }>('/analysis/gems/names', {
		q: query,
		limit: '15',
	});
	return resp.names || [];
}

export async function fetchCompare(gems: string[], variant: string): Promise<CompareGem[]> {
	const resp = await get<{ count: number; data: any[] }>('/analysis/compare', {
		gems: gems.join(','),
		variant,
	});
	const results = (resp.data || []).map(mapCompareRow);

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

export function connectMercure(onUpdate: () => void, onConnectionChange?: (connected: boolean) => void): MercureConnection {
	const state: MercureConnection = { close: () => {}, connected: false };
	let eventSource: EventSource | null = null;
	let tokenTimeout: ReturnType<typeof setTimeout> | null = null;
	let retries = 0;

	function retryDelay(): number {
		// Exponential backoff: 2s, 4s, 8s, capped at 10s (fast recovery after deploys)
		return Math.min(2000 * Math.pow(2, retries), 10000);
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
		try {
			const tokenResp = await get<{ token: string; url: string }>('/mercure/token');
			const { token, url } = tokenResp;
			retries = 0;

			closeEventSource();

			const authedUrl = new URL(url);
			authedUrl.searchParams.set('topic', 'poe/analysis/updated');
			authedUrl.searchParams.set('authorization', token);

			eventSource = new EventSource(authedUrl.toString());

			eventSource.onopen = () => {
				state.connected = true;
				onConnectionChange?.(true);
				retries = 0;
			};

			eventSource.onmessage = (msg) => {
				let parsed: any;
				try {
					parsed = JSON.parse(msg.data);
				} catch {
					onUpdate();
					return;
				}
				if (parsed.type === 'waiting' || parsed.type === 'ready' || parsed.type === 'error') {
					dispatchTradeEvent(parsed);
					return;
				}
				onUpdate();
			};

			eventSource.onerror = () => {
				// Close explicitly — prevent browser's native EventSource reconnect
				// from fighting our token-refresh retry logic.
				closeEventSource();
				state.connected = false;
				onConnectionChange?.(false);
				if (tokenTimeout) clearTimeout(tokenTimeout);
				retries++;
				tokenTimeout = setTimeout(connect, retryDelay());
			};

			// Token TTL is 30min — refresh before expiry
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, 25 * 60 * 1000);
		} catch (err) {
			console.warn('[Mercure] Connection failed, retrying in', retryDelay() / 1000, 's:', err);
			state.connected = false;
			onConnectionChange?.(false);
			retries++;
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, retryDelay());
		}
	}

	state.close = () => {
		if (eventSource) eventSource.close();
		if (tokenTimeout) clearTimeout(tokenTimeout);
		state.connected = false;
				onConnectionChange?.(false);
	};

	connect();
	return state;
}
