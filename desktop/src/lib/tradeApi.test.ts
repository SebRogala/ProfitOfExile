import { describe, it, expect } from 'vitest';
import {
	isSource,
	mercListingRow,
	toListingRow,
	type MercTradeListing,
	type TradeListingDetail,
	type TradeQueueEvent
} from './tradeApi';

/**
 * `TradeListings.svelte` renders one row type for two very different lookups,
 * so what these tests pin is the mapping into that shared row — the seam where
 * a gem's `price` becomes an `amount` and a mercenary listing keeps its own.
 *
 * Every fixture field below holds a value no other field holds. A crossed wire
 * (`chaosPrice` where `amount` belongs, the account where the timestamp does)
 * then shows up as a wrong value rather than passing on a coincidence.
 */

function gemDetail(overrides: Partial<TradeListingDetail> = {}): TradeListingDetail {
	return {
		price: 5,
		currency: 'divine',
		chaosPrice: 1250,
		account: 'SellerOne',
		indexedAt: '2026-08-26T01:00:00Z',
		gemLevel: 21,
		gemQuality: 23,
		corrupted: true,
		...overrides
	};
}

function mercListing(overrides: Partial<MercTradeListing> = {}): MercTradeListing {
	return {
		chaosPrice: 640,
		currency: 'exalted',
		amount: 8,
		account: 'SellerTwo',
		indexedAt: '2026-08-25T22:30:00Z',
		...overrides
	};
}

describe('toListingRow', () => {
	/**
	 * The rename is the whole point of this function: the gem lookup calls the
	 * seller's ask `price`, the shared row calls it `amount`, and `chaosPrice`
	 * is a separate normalised number that must not stand in for it.
	 */
	it('maps a gem listing detail onto the shared row', () => {
		expect(toListingRow(gemDetail())).toEqual({
			chaosPrice: 1250,
			currency: 'divine',
			amount: 5,
			account: 'SellerOne',
			indexedAt: '2026-08-26T01:00:00Z'
		});
	});

	/**
	 * The gem-only fields are the caller's business, rendered through the detail
	 * snippet. Letting them ride along in the row (a spread instead of a
	 * mapping) would put gem vocabulary into a component the mercenary page also
	 * feeds, which is the drift `TradeListings` was extracted to prevent.
	 */
	it('leaves the gem-specific fields out of the shared row', () => {
		const row = toListingRow(gemDetail());
		expect(Object.keys(row).sort()).toEqual([
			'account',
			'amount',
			'chaosPrice',
			'currency',
			'indexedAt'
		]);
	});
});

describe('mercListingRow', () => {
	/**
	 * A mercenary listing already carries `amount` separately from
	 * `chaosPrice`, and both reach the row: the page quotes the raw ask because
	 * it has no divine rate to undo the chaos normalisation with.
	 */
	it('maps a mercenary listing onto the shared row', () => {
		expect(mercListingRow(mercListing())).toEqual({
			chaosPrice: 640,
			currency: 'exalted',
			amount: 8,
			account: 'SellerTwo',
			indexedAt: '2026-08-25T22:30:00Z'
		});
	});
});

/**
 * One queue serves the gem comparator and the mercenary capture, and every
 * event carries the tag saying whose it is. The Comparator and its overlay both
 * filter through this helper so they cannot drift apart on the rule — a gem
 * surface that stopped filtering would render the mercenary search's progress
 * as its own.
 */
describe('isSource', () => {
	function queued(source: TradeQueueEvent['source']): TradeQueueEvent {
		return { kind: 'queued', source, gem: 'Ice Shot', position: 1, total: 3 };
	}

	it('accepts an event tagged with the source being watched', () => {
		expect(isSource(queued('gem'), 'gem')).toBe(true);
	});

	it('rejects a mercenary event on a gem surface', () => {
		expect(isSource(queued('mercenary'), 'gem')).toBe(false);
	});

	it('rejects a gem event on a mercenary surface', () => {
		expect(isSource(queued('gem'), 'mercenary')).toBe(false);
	});

	/**
	 * `cancelled` is the one variant with no `gem` field — it reports a
	 * remaining count instead — so it is the variant most likely to lose its
	 * tag, and the one that would clear another window's queue display if it
	 * slipped through.
	 */
	it('rejects a cancellation belonging to the other source', () => {
		const cancelled: TradeQueueEvent = { kind: 'cancelled', source: 'mercenary', remaining: 2 };
		expect(isSource(cancelled, 'gem')).toBe(false);
	});
});
