import type { TradeLookupResult } from '$lib/tradeApi';
import { hasTradeListings } from '$lib/tradeApi';

/** Select a built quote; the trade-price definition belongs to the result builders. */
export function comparatorPrice(
	transPrice: number,
	noData: boolean,
	trade: (Pick<TradeLookupResult, 'medianTop3'> & { listings: ReadonlyArray<unknown> }) | null | undefined
): { label: 'price' | 'ninja'; chaos: number } {
	if (trade && hasTradeListings(trade)) return { label: 'price', chaos: trade.medianTop3 };
	return { label: 'ninja', chaos: noData ? NaN : transPrice };
}
