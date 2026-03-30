/**
 * Trade API types — desktop edition.
 * Type definitions for trade data received from the Go backend via compare results
 * and from Rust trade_lookup commands.
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
	divinePrice: number;
	tradeUrl: string;
	fetchedAt: string;
}

export interface TradeListingDetail {
	price: number;
	currency: string;
	chaosPrice: number;
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
