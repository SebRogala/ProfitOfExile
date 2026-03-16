/**
 * Trade API client — lookups against the GGG trade API via our backend gate.
 * Includes a Mercure event listener registry for async wait/ready/error events.
 */

// --- Types matching backend API contract ---

export interface TradeLookupResult {
	gem: string;
	variant: string;
	total: number;
	priceFloor: number;
	priceCeiling: number;
	priceSpread: number;
	medianTop10: number;
	listings: TradeListingDetail[];
	signals: TradeSignals;
	fetchedAt: string;
}

export interface TradeListingDetail {
	price: number;
	currency: string;
	account: string;
	indexedAt: string;
	gemLevel: number;
	gemQuality: number;
	corrupted: boolean;
}

export interface TradeSignals {
	sellerConcentration: 'NORMAL' | 'CONCENTRATED' | 'MONOPOLY';
	cheapestStaleness: 'FRESH' | 'AGING' | 'STALE';
	priceOutlier: boolean;
	uniqueAccounts: number;
}

// --- Lookup response ---

export interface LookupResponse {
	/** Populated when the gate returns data within the sync budget (200). */
	immediate: TradeLookupResult | null;
	/** Populated when the request was queued for async processing (202). */
	requestId: string | null;
}

// --- Trade event listener registry ---

export type TradeListener = {
	onWait?: (seconds: number) => void;
	onReady?: (data: TradeLookupResult) => void;
	onError?: (msg: string) => void;
};

const tradeListeners = new Map<string, TradeListener>();

/**
 * Register callbacks for a queued trade lookup identified by requestId.
 * Returns an unsubscribe function.
 */
export function registerTradeListener(requestId: string, listener: TradeListener): () => void {
	tradeListeners.set(requestId, listener);
	return () => {
		tradeListeners.delete(requestId);
	};
}

/**
 * Dispatch an incoming Mercure trade event to the registered listener.
 * Called from the Mercure onmessage handler in api.ts.
 */
export function dispatchTradeEvent(event: { type: string; requestId: string; [key: string]: any }): void {
	const listener = tradeListeners.get(event.requestId);
	if (!listener) return;

	switch (event.type) {
		case 'waiting':
			listener.onWait?.(event.waitSeconds ?? 0);
			break;
		case 'ready':
			listener.onReady?.(event.data as TradeLookupResult);
			break;
		case 'error':
			listener.onError?.(event.message ?? 'Unknown error');
			break;
	}
}

// --- HTTP lookup ---

const API_BASE = '/api';

/**
 * Fire a trade lookup for a gem+variant.
 * - 200 → immediate result (cache hit or fast gate response)
 * - 202 → queued; registers Mercure listener AND starts polling as fallback
 *
 * The polling fallback handles cases where Mercure SSE is not connected
 * (e.g., missing MERCURE_SUBSCRIBER_KEY in dev). It retries the same
 * request every 3s (max 10 attempts) — once the gate completes, the
 * result is cached and the retry returns 200 immediately.
 */
export async function lookupTrade(gem: string, variant: string, force = false): Promise<LookupResponse> {
	const resp = await fetch(`${API_BASE}/trade/lookup`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ gem, variant, force }),
	});

	if (resp.status === 200) {
		const data: TradeLookupResult = await resp.json();
		return { immediate: data, requestId: null };
	}

	if (resp.status === 202) {
		const body: { requestId: string } = await resp.json();
		return { immediate: null, requestId: body.requestId };
	}

	const text = await resp.text().catch(() => '');
	throw new Error(`Trade lookup failed: ${resp.status} ${resp.statusText} ${text}`);
}

/**
 * Poll for a trade lookup result by retrying the same request until cache is hit.
 * Used as fallback when Mercure SSE is not available.
 */
export async function pollTradeResult(
	gem: string,
	variant: string,
	maxAttempts = 10,
	intervalMs = 3000,
): Promise<TradeLookupResult | null> {
	for (let i = 0; i < maxAttempts; i++) {
		await new Promise((r) => setTimeout(r, intervalMs));
		try {
			const { immediate } = await lookupTrade(gem, variant);
			if (immediate) return immediate;
		} catch {
			// ignore errors during polling
		}
	}
	return null;
}
