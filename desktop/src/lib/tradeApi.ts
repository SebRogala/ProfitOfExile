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

// --- Mercenary trade result (mirrors Rust MercTradeResult) ---
// Desktop-only: the server knows nothing about mercenary listings, so unlike
// TradeLookupResult these types mirror no Go struct.

export interface MercTradeListing {
	chaosPrice: number;
	currency: string;
	amount: number;
	account: string;
	indexedAt: string;
}

export interface MercTradeResult {
	queryHash: string;
	league: string;
	total: number;
	listings: MercTradeListing[];
	floorChaos: number;
	medianChaos: number;
	fetchedAtMs: number;
	/** The complexity budget forced the query to drop support cells the capture
	 *  read, so it asks a looser question than the panel showed. */
	truncated: boolean;
}

// --- Trade queue events (mirrors Rust TradeQueueEvent) ---

/** Which consumer a queued lookup belongs to (mirrors Rust `TradeSource`). */
export type TradeSource = 'gem' | 'mercenary';

export type TradeQueueEvent =
	| { kind: 'queued'; source: TradeSource; gem: string; position: number; total: number }
	| { kind: 'waiting'; source: TradeSource; gem: string; waitSecs: number; position: number; total: number }
	| { kind: 'fetching'; source: TradeSource; gem: string; position: number; total: number }
	| { kind: 'done'; source: TradeSource; gem: string }
	| { kind: 'error'; source: TradeSource; gem: string; error: string }
	| { kind: 'cancelled'; source: TradeSource; remaining: number };

/**
 * One queue, several consumers: every listener must drop the events that are
 * not its own or it renders another window's progress. Shared so the two gem
 * surfaces (Comparator and its overlay) cannot drift apart on the rule.
 */
export function isSource(event: TradeQueueEvent, source: TradeSource): boolean {
	return event.source === source;
}

export interface TradeQueueDisplay {
	gem?: string;
	position: number;
	total: number;
	status: 'queued' | 'waiting' | 'fetching';
	waitSecs: number;
}

// --- Listings table row ---

/**
 * One row of the shared listings table — the fields both a gem listing and a
 * mercenary listing carry. Anything type-specific (a gem's level/quality) is
 * rendered by the caller's detail snippet, keyed by row index.
 */
export interface TradeListingRow {
	/**
	 * Price normalized to chaos — but only where a rate existed to normalize
	 * with. A gem row's is a real conversion (the Comparator reads a divine
	 * rate off the market page); a mercenary row's is not, because Rust
	 * normalizes merc listings with a rate of 0 and hands back the seller's own
	 * `amount`. Cross-currency comparison on this field is only meaningful for
	 * gem rows.
	 */
	chaosPrice: number;
	/** Raw seller currency. */
	currency: string;
	/** Raw seller amount in `currency`. */
	amount: number;
	account: string;
	indexedAt: string;
}

export function toListingRow(detail: TradeListingDetail): TradeListingRow {
	return {
		chaosPrice: detail.chaosPrice,
		currency: detail.currency,
		amount: detail.price,
		account: detail.account,
		indexedAt: detail.indexedAt
	};
}

export function mercListingRow(listing: MercTradeListing): TradeListingRow {
	return {
		chaosPrice: listing.chaosPrice,
		currency: listing.currency,
		amount: listing.amount,
		account: listing.account,
		indexedAt: listing.indexedAt
	};
}

/**
 * How a seller's raw ask is written — the one place, because the same number
 * is printed twice on one card: once in the mercenary headline ("from 5
 * divine") and once in the row below it. A trailing `.0` on every whole price
 * reads as precision the listing does not have.
 *
 * NOT `$lib/price.svelte.ts`: that formats amounts the app COMPUTED in chaos
 * and picks its own unit. A listing carries the currency its seller asked in,
 * and rewriting either half misreports what is on offer.
 */
export function formatListingAmount(amount: number): string {
	return Number.isInteger(amount) ? amount.toString() : amount.toFixed(1);
}

