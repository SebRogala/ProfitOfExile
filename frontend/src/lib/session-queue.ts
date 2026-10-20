/**
 * Session queue pricing: the trade floor a queued gem was picked at, the floor
 * a refresh found, and the change between them.
 */
import { hasTradeListings, type TradeLookupResult } from './tradeApi';

/**
 * A trade floor as the queue records it. A lookup that matched no listings is
 * `{ listed: false }`, never a floor of 0: its zeros mean "no price", and a 0c
 * snapshot would read as a real price and turn the next refresh into a delta.
 */
export type FloorQuote =
	| { listed: true; floor: number; floorOriginal: number; currency: string }
	| { listed: false };

export interface QueueItem {
	gem: string;
	variant: string;
	pickedAt: Date;
	snapshotROI: number;
	/** The floor when the gem was queued; null when no lookup had landed yet. */
	snapshot: FloorQuote | null;
	snapshotDivineRate: number;
	/** The floor the last refresh found; absent until a refresh lands. */
	current?: FloorQuote;
	/** current − snapshot in chaos; null unless both sides were priced. */
	priceDelta?: number | null;
	recommendation?: string;
	refreshing?: boolean;
}

/** The floor a lookup quotes, in chaos and in the currency of its cheapest listing. */
export function floorQuote(lookup: TradeLookupResult): FloorQuote {
	if (!hasTradeListings(lookup)) return { listed: false };
	const cheapest = lookup.listings[0];
	return {
		listed: true,
		floor: lookup.priceFloor,
		floorOriginal: cheapest.price,
		currency: cheapest.currency,
	};
}

/** The chaos change from `snapshot` to `current`; null when either side has no price. */
export function quoteDelta(current: FloorQuote | null | undefined, snapshot: FloorQuote | null | undefined): number | null {
	if (!current?.listed || !snapshot?.listed) return null;
	return current.floor - snapshot.floor;
}

export function fmtPrice(v: number): string {
	return Number.isInteger(v) ? v.toString() : v.toFixed(1);
}

/** A queue cell's price text: an em dash before any lookup, "no listings" for an empty one. */
export function formatQuote(quote: FloorQuote | null | undefined): string {
	if (!quote) return '—';
	if (!quote.listed) return 'no listings';
	if (quote.currency === 'divine') {
		return `${fmtPrice(quote.floorOriginal)} div (${fmtPrice(quote.floor)}c)`;
	}
	return `${fmtPrice(quote.floor)}c`;
}
