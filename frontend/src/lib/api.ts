/**
 * Lab Dashboard API service.
 * Fetches real data from the Go backend at /api/*.
 */

// --- Types ---

export interface StatusData {
	lastUpdate: string;
	nextFetch: string;
	connected: boolean;
	collectorUptime: string;
}

export type PriceTier = 'TOP' | 'MID' | 'LOW' | '';
export type SellUrgency = 'SELL_NOW' | 'UNDERCUT' | 'HOLD' | 'WAIT' | '';
export type SellabilityLabel = 'FAST SELL' | 'GOOD' | 'MODERATE' | 'SLOW' | 'UNLIKELY' | '';

export interface GemPlay {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	roi: number;
	roiPercent: number;
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
	tierAction: string;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
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
}

export interface FontEVData {
	colors: FontColor[];
	qualityAvgRoi: number;
	bestColor: string;
	bestAdvantage: number;
}

export interface WindowAlert {
	windowSignal: string;
	name: string;
	variant: string;
	roi: number;
	transListings: number;
	baseListings: number;
	baseVelocity: number;
	priceVelocity: number;
	listingVelocity: number;
	liquidityTier: string;
	action: string;
	signal: string;
	priceTier: PriceTier;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
	priceTrend: number[];
	listingsTrend: number[];
	baseListingsTrend: number[];
	trendUnavailable: boolean;
	history: SignalTransition[];
}

export interface MarketOverviewData {
	avgTransPrice: number;
	avgTransPriceDelta: number;
	avgBaseListings: number;
	avgBaseListingsDelta: number;
	activeGems: number;
	weekendPremium: number;
	windowGems: number;
	windowBreakdown: Record<string, number>;
	trapGems: number;
	mostVolatileColor: string;
	mostVolatileCV: number;
	mostStableColor: string;
	mostStableCV: number;
}

export interface CompareGem {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	roi: number;
	roiPercent: number;
	signal: string;
	cv: number;
	transListings: number;
	transVelocity: number;
	baseListings: number;
	baseVelocity: number;
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
		tierAction: r.tierAction || '',
		sellUrgency: (r.sellUrgency || '') as SellUrgency,
		sellReason: r.sellReason || '',
		sellability: r.sellability || 0,
		sellabilityLabel: (r.sellabilityLabel || '') as SellabilityLabel,
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
		signal: r.signal || '',
		cv: Math.round(r.cv || 0),
		transListings: r.transListings || r.transfiguredListings || 0,
		transVelocity: Math.round(r.priceVelocity || 0),
		baseListings: r.baseListings || 0,
		baseVelocity: Math.round(r.listingVelocity || 0),
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
		};
	} catch (err) {
		console.warn('[Status] Failed to fetch status:', err);
		return {
			lastUpdate: '',
			nextFetch: '',
			connected: false,
			collectorUptime: '',
		};
	}
}

export async function fetchBestPlays(
	variant?: string,
	budget?: number,
	sort?: 'roi' | 'roiPercent'
): Promise<GemPlay[]> {
	const params: Record<string, string> = {};
	if (variant) params.variant = variant;
	if (budget) params.budget = String(budget);
	if (sort === 'roiPercent') params.sort = 'pct';

	const resp = await get<{ count: number; data: any[] }>('/analysis/collective', params);
	return (resp.data || []).map(mapCollectiveRow);
}

export async function fetchVariantPlays(variant: string): Promise<GemPlay[]> {
	return fetchBestPlays(variant);
}

export async function fetchFontEV(variant: string): Promise<FontEVData> {
	const params: Record<string, string> = {};
	if (variant) params.variant = variant;

	const resp = await get<{ count: number; data: any[] }>('/analysis/font', params);
	const rows = resp.data || [];

	const colors: FontColor[] = rows.map((r: any) => ({
		color: r.color || '',
		ev: Math.round(r.ev || 0),
		pool: r.pool || 0,
		winners: r.winners || 0,
		pWin: Math.round((r.pWin || 0) * 10000) / 100,
		profit: Math.round(r.profit || 0),
		evDelta2h: 0, // not in backend yet
	}));

	// Compute best color
	const sorted = [...colors].sort((a, b) => b.ev - a.ev);
	const best = sorted[0];
	const second = sorted[1];

	return {
		colors,
		qualityAvgRoi: 0, // needs quality data cross-reference
		bestColor: best?.color || '',
		bestAdvantage: best && second ? Math.round(best.ev - second.ev) : 0,
	};
}

export async function fetchWindowAlerts(): Promise<WindowAlert[]> {
	// Window alerts are derived from trends data — gems with active window signals
	const resp = await get<{ count: number; data: any[] }>('/analysis/trends', { limit: '20' });
	const rows = resp.data || [];

	const alerts = rows
		.filter((r: any) => ['BREWING', 'OPENING', 'OPEN', 'CLOSING'].includes(r.windowSignal))
		.map((r: any) => ({
			windowSignal: r.windowSignal || '',
			name: r.name || '',
			variant: r.variant || '',
			roi: Math.round(r.currentPrice || 0),
			transListings: r.currentListings || 0,
			baseListings: r.baseListings || 0,
			baseVelocity: Math.round(r.baseVelocity || 0),
			priceVelocity: Math.round((r.priceVelocity || 0) * 10) / 10,
			listingVelocity: Math.round(r.listingVelocity || 0),
			liquidityTier: r.liquidityTier || '',
			action: r.tierAction || r.sellReason || '',
			signal: r.signal || '',
			priceTier: (r.priceTier || '') as PriceTier,
			sellUrgency: (r.sellUrgency || '') as SellUrgency,
			sellReason: r.sellReason || '',
			sellability: r.sellability || 0,
			sellabilityLabel: (r.sellabilityLabel || '') as SellabilityLabel,
			priceTrend: Array.isArray(r.priceTrend) ? r.priceTrend : [],
			listingsTrend: Array.isArray(r.listingsTrend) ? r.listingsTrend : [],
			baseListingsTrend: Array.isArray(r.baseListingsTrend) ? r.baseListingsTrend : [],
			trendUnavailable: r.windowSignal && !r.priceTrend?.length,
			history: [],
		}))
		.sort((a: WindowAlert, b: WindowAlert) => {
			const order = ['OPEN', 'OPENING', 'BREWING', 'CLOSING'];
			return order.indexOf(a.windowSignal) - order.indexOf(b.windowSignal);
		});

	return alerts;
}

export async function fetchMarketOverview(): Promise<MarketOverviewData> {
	// Aggregated from trends data client-side
	const resp = await get<{ count: number; data: any[] }>('/analysis/trends', { limit: '500' });
	const rows = resp.data || [];

	if (rows.length === 0) {
		return {
			avgTransPrice: 0, avgTransPriceDelta: 0,
			avgBaseListings: 0, avgBaseListingsDelta: 0,
			activeGems: 0, weekendPremium: 0,
			windowGems: 0, windowBreakdown: {},
			trapGems: 0,
			mostVolatileColor: '', mostVolatileCV: 0,
			mostStableColor: '', mostStableCV: 0,
		};
	}

	const avgPrice = rows.reduce((s: number, r: any) => s + (r.currentPrice || 0), 0) / rows.length;
	const avgListings = rows.reduce((s: number, r: any) => s + (r.baseListings || 0), 0) / rows.length;

	const windowBreakdown: Record<string, number> = {};
	let windowGems = 0;
	let trapGems = 0;
	for (const r of rows) {
		if (['BREWING', 'OPENING', 'OPEN', 'CLOSING'].includes(r.windowSignal)) {
			windowGems++;
			windowBreakdown[r.windowSignal] = (windowBreakdown[r.windowSignal] || 0) + 1;
		}
		if (r.signal === 'TRAP') trapGems++;
	}

	// CV by color
	const colorCV: Record<string, number[]> = {};
	for (const r of rows) {
		if (r.gemColor && r.cv != null) {
			if (!colorCV[r.gemColor]) colorCV[r.gemColor] = [];
			colorCV[r.gemColor].push(r.cv);
		}
	}
	const colorAvgCV = Object.entries(colorCV).map(([color, cvs]) => ({
		color,
		avgCV: Math.round(cvs.reduce((s, v) => s + v, 0) / cvs.length),
	}));
	colorAvgCV.sort((a, b) => b.avgCV - a.avgCV);
	const mostVolatile = colorAvgCV[0];
	const mostStable = colorAvgCV[colorAvgCV.length - 1];

	return {
		avgTransPrice: Math.round(avgPrice),
		avgTransPriceDelta: 0, // would need historical comparison
		avgBaseListings: Math.round(avgListings),
		avgBaseListingsDelta: 0,
		activeGems: rows.length,
		weekendPremium: 0,
		windowGems,
		windowBreakdown,
		trapGems,
		mostVolatileColor: mostVolatile?.color || '',
		mostVolatileCV: mostVolatile?.avgCV || 0,
		mostStableColor: mostStable?.color || '',
		mostStableCV: mostStable?.avgCV || 0,
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
	await Promise.allSettled(
		results.map(async (gem) => {
			gem.signalHistory = await fetchSignalHistory(gem.name, gem.variant);
		})
	);

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

export function connectMercure(onUpdate: () => void): MercureConnection {
	const state: MercureConnection = { close: () => {}, connected: false };
	let eventSource: EventSource | null = null;
	let tokenTimeout: ReturnType<typeof setTimeout> | null = null;
	let retries = 0;
	const MAX_RETRIES = 3;

	async function connect() {
		if (retries >= MAX_RETRIES) return; // Stop trying if Mercure isn't available

		try {
			const tokenResp = await get<{ token: string; url: string }>('/mercure/token');
			const { token, url } = tokenResp;
			retries = 0; // Reset on success

			if (eventSource) eventSource.close();

			const authedUrl = new URL(url);
			authedUrl.searchParams.set('topic', 'poe/analysis/*');
			authedUrl.searchParams.set('authorization', token);

			eventSource = new EventSource(authedUrl.toString());

			eventSource.onopen = () => {
				state.connected = true;
			};

			eventSource.onmessage = () => {
				onUpdate();
			};

			eventSource.onerror = () => {
				state.connected = false;
				if (tokenTimeout) clearTimeout(tokenTimeout);
				retries++;
				if (retries < MAX_RETRIES) {
					tokenTimeout = setTimeout(connect, 5000 * retries);
				}
			};

			// Token TTL is 30min — refresh before expiry
			if (tokenTimeout) clearTimeout(tokenTimeout);
			tokenTimeout = setTimeout(connect, 25 * 60 * 1000);
		} catch (err) {
			console.warn('[Mercure] Connection failed:', err);
			state.connected = false;
			retries++;
			if (retries < MAX_RETRIES) {
				if (tokenTimeout) clearTimeout(tokenTimeout);
				tokenTimeout = setTimeout(connect, 10000);
			}
		}
	}

	state.close = () => {
		if (eventSource) eventSource.close();
		if (tokenTimeout) clearTimeout(tokenTimeout);
		state.connected = false;
	};

	connect();
	return state;
}
