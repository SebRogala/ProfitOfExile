/**
 * Trade wire types shared by the web and desktop trade clients: the lookup
 * result the backend returns, and the guard for a lookup that found nothing.
 */

export interface TradeLookupResult {
	gem: string;
	variant: string;
	league: string;
	total: number;
	priceFloor: number;
	priceCeiling: number;
	priceSpread: number;
	medianTop10: number;
	/** Trade price; normative definition: docs/TRADE-LIFECYCLE.md#trade-price-definition. */
	medianTop3: number;
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

/**
 * Whether a trade lookup found anything to price. A search that matched no
 * listings still comes back as a successful result, with `listings: []` and
 * every price field (floor, median, ceiling, spread) at 0 and the signals at
 * their defaults. Those zeros mean "no price", not "worth 0c", so callers
 * gate every price, delta and signal on this before showing or storing it.
 */
export function hasTradeListings(lookup: { listings?: ReadonlyArray<unknown> | null } | null | undefined): boolean {
	return (lookup?.listings?.length ?? 0) > 0;
}
