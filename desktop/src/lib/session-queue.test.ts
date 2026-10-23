import { describe, it, expect } from 'vitest';
import { floorQuote, formatQuote, quoteDelta, type FloorQuote } from './session-queue';
import type { TradeListingDetail, TradeLookupResult } from './tradeApi';

function listing(overrides: Partial<TradeListingDetail> = {}): TradeListingDetail {
	return {
		price: 2,
		currency: 'divine',
		chaosPrice: 480,
		account: 'Seller',
		indexedAt: '2026-10-20T10:00:00Z',
		gemLevel: 20,
		gemQuality: 20,
		corrupted: false,
		...overrides,
	};
}

/**
 * A lookup as the server sends it. With no listings it is the shape the
 * server returns for a search that matched nothing: every price at 0 and the
 * signals at their defaults.
 */
function lookup(listings: TradeListingDetail[], priceFloor = 0): TradeLookupResult {
	return {
		gem: 'Kinetic Blast of Clustering',
		variant: '20/20',
		total: listings.length,
		priceFloor,
		priceCeiling: 0,
		priceSpread: 0,
		medianTop10: 0,
		medianTop3: 0,
		listings,
		signals: { sellerConcentration: 'NORMAL', cheapestStaleness: 'FRESH', priceOutlier: false, uniqueAccounts: 0 },
		divinePrice: 240,
		tradeUrl: '',
		fetchedAt: '2026-10-20T10:00:00Z',
	};
}

const priced = (floor: number): FloorQuote => ({ listed: true, floor, floorOriginal: floor, currency: 'chaos' });

describe('floorQuote', () => {
	it('records an empty lookup as unlisted rather than as a 0c floor', () => {
		expect(floorQuote(lookup([]))).toEqual({ listed: false });
	});

	it('quotes a listed lookup at its chaos floor in the currency of its cheapest listing', () => {
		expect(floorQuote(lookup([listing(), listing({ price: 3, chaosPrice: 720 })], 481))).toEqual({
			listed: true,
			floor: 481,
			floorOriginal: 2,
			currency: 'divine',
		});
	});
});

describe('quoteDelta', () => {
	it('is the chaos change from the snapshot floor to the current one', () => {
		expect(quoteDelta(priced(30), priced(45))).toBe(-15);
	});

	/** The false drop a refresh used to report: 0 − 45 read as a 45c fall. */
	it('is null when the refresh found no listings', () => {
		expect(quoteDelta({ listed: false }, priced(45))).toBeNull();
	});

	it('is null when the snapshot had no listings', () => {
		expect(quoteDelta(priced(30), { listed: false })).toBeNull();
	});

	it('is null when the gem was queued before any lookup landed', () => {
		expect(quoteDelta(priced(30), null)).toBeNull();
	});
});

describe('formatQuote', () => {
	it('says "no listings" for an unlisted quote', () => {
		expect(formatQuote({ listed: false })).toBe('no listings');
	});

	it('shows an em dash before any lookup has landed', () => {
		expect(formatQuote(null)).toBe('—');
	});

	it('shows a divine quote in divines with its chaos floor alongside', () => {
		expect(formatQuote({ listed: true, floor: 481, floorOriginal: 2, currency: 'divine' })).toBe('2 div (481c)');
	});

	it('shows a chaos quote as its chaos floor', () => {
		expect(formatQuote({ listed: true, floor: 45.5, floorOriginal: 45.5, currency: 'chaos' })).toBe('45.5c');
	});
});
