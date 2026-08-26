import { describe, it, expect } from 'vitest';
import {
	MERC_TRADE_MAX_SEARCHES,
	tradeHeadline,
	tradeStatusLabel,
	tradeStatusTone
} from './trade-view';
import type { MercTradeState, MercTradeStatus } from './capture';
import type { MercTradeListing, MercTradeResult } from '$lib/tradeApi';

/**
 * Every status, listed once so the compiler is the thing that notices a new
 * one: adding a variant to `MercTradeStatus` without adding it here fails
 * `npm run check`, which is the only way a "distinct label per status" test can
 * keep meaning anything as the enum grows.
 */
const STATUS_PRESENCE: Record<MercTradeStatus, true> = {
	off: true,
	idle: true,
	'waiting-league': true,
	queued: true,
	searching: true,
	done: true,
	error: true
};
const ALL_STATUSES = Object.keys(STATUS_PRESENCE) as MercTradeStatus[];

/**
 * A slice with nothing retained. Every test that wants a result, a url or a
 * spent budget puts it there explicitly, so a headline that appears without
 * one is the production code inventing it.
 */
function state(overrides: Partial<MercTradeState> = {}): MercTradeState {
	return {
		status: 'idle',
		queryHash: null,
		url: null,
		result: null,
		error: null,
		searchesUsed: 0,
		...overrides
	};
}

/**
 * Distinct value per field on purpose — `amount` is not `chaosPrice` and
 * neither is `floorChaos`, so a headline that quotes the wrong one prints a
 * number that appears nowhere else in the fixture.
 */
function listing(overrides: Partial<MercTradeListing> = {}): MercTradeListing {
	return {
		chaosPrice: 1250,
		currency: 'divine',
		amount: 5,
		account: 'SellerOne',
		indexedAt: '2026-08-26T01:00:00Z',
		...overrides
	};
}

function result(overrides: Partial<MercTradeResult> = {}): MercTradeResult {
	return {
		queryHash: 'a1b2c3',
		league: 'Mirage',
		total: 4,
		listings: [listing()],
		floorChaos: 1250,
		medianChaos: 1400,
		fetchedAtMs: 1_700_000_000_000,
		truncated: false,
		...overrides
	};
}

describe('tradeStatusLabel', () => {
	/**
	 * The badge is the only place the user can tell "not searching" from
	 * "cannot search yet" from "waiting its turn". Two statuses sharing a label
	 * would make a bounded wait read as a dead feature, which is exactly what
	 * collapsing two `case` arms into one would do.
	 */
	it('gives every status a label no other status uses', () => {
		const labels = ALL_STATUSES.map((status) => tradeStatusLabel(state({ status })));
		expect(new Set(labels).size).toBe(ALL_STATUSES.length);
	});

	it.each([
		['off', 'module off'],
		['idle', 'not searching'],
		['waiting-league', 'waiting for league'],
		['queued', 'queued'],
		['searching', 'searching'],
		['done', 'search done']
	] as [MercTradeStatus, string][])('labels %s as "%s"', (status, expected) => {
		expect(tradeStatusLabel(state({ status }))).toBe(expected);
	});

	/**
	 * Retiring a capture mid-search is the user ending the question, not a
	 * break — Rust marks it with the `cancelled` error string, and the badge has
	 * to say so rather than send them hunting for a failure that did not happen.
	 */
	it('labels a cancelled error as a cancellation rather than a failure', () => {
		expect(tradeStatusLabel(state({ status: 'error', error: 'cancelled' }))).toBe(
			'search cancelled'
		);
	});

	it('labels any other error as a failure', () => {
		expect(tradeStatusLabel(state({ status: 'error', error: 'rate limited' }))).toBe(
			'search failed'
		);
	});
});

describe('tradeStatusTone', () => {
	/**
	 * The tone half of the same discrimination: painting a cancellation red is
	 * what sends a user looking for a break. `muted` is the claim that nothing
	 * went wrong.
	 */
	it('tones a cancelled error muted rather than as a failure', () => {
		const tone = tradeStatusTone(state({ status: 'error', error: 'cancelled' }));
		expect(tone).toBe('muted');
		expect(tone).not.toBe('fail');
	});

	it('tones any other error as a failure', () => {
		expect(tradeStatusTone(state({ status: 'error', error: 'rate limited' }))).toBe('fail');
	});

	it('tones a finished search as a pass', () => {
		expect(tradeStatusTone(state({ status: 'done', result: result() }))).toBe('pass');
	});

	/**
	 * A missing league is the app waiting on the game, not failing — `unknown`
	 * is the tone that says "no answer yet" without accusing anything.
	 */
	it.each(['waiting-league', 'queued', 'searching'] as MercTradeStatus[])(
		'tones %s as an unfinished answer, not a failure',
		(status) => {
			expect(tradeStatusTone(state({ status }))).toBe('unknown');
		}
	);
});

describe('tradeHeadline', () => {
	/**
	 * "none found" is a claim about the market, so it is reserved for GGG's own
	 * count being zero. Anything else — a fetched page that came back empty
	 * included — must not make it.
	 */
	it('says none found when the query matched nothing', () => {
		const headline = tradeHeadline(state({ status: 'done', result: result({ total: 0, listings: [] }) }));
		expect(headline).toBe('none found');
	});

	it('counts a single match in the singular', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ total: 1 }) })
		);
		expect(headline).toBe('1 listing · from 5 divine');
	});

	it('counts several matches in the plural', () => {
		const headline = tradeHeadline(state({ status: 'done', result: result({ total: 4 }) }));
		expect(headline).toBe('4 listings · from 5 divine');
	});

	/**
	 * `total` is GGG's count for the whole query; `listings` is only the page
	 * that was fetched. Counting the rows instead would tell a user there are
	 * two of an item the site says there are 87 of.
	 */
	it('takes the count from the query total, not from the fetched page', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ total: 87, listings: [listing(), listing()] }) })
		);
		expect(headline).toMatch(/^87 listings/);
	});

	/**
	 * The price is the cheapest seller's RAW ask. `floorChaos` is a chaos
	 * normalisation and this page has no divine rate to undo it with, so
	 * quoting it would print a number no seller ever asked for.
	 */
	it('quotes the cheapest listing raw seller price, not its chaos value', () => {
		const headline = tradeHeadline(
			state({
				status: 'done',
				result: result({ listings: [listing({ amount: 5, currency: 'divine', chaosPrice: 1250 })] })
			})
		);
		expect(headline).toContain('from 5 divine');
		expect(headline).not.toContain('1250');
	});

	/** `price asc` means row 0 IS the floor — a later row must not be the one quoted. */
	it('quotes the first listing, which the price-ascending query makes the cheapest', () => {
		const headline = tradeHeadline(
			state({
				status: 'done',
				result: result({
					listings: [listing({ amount: 5, currency: 'divine' }), listing({ amount: 9, currency: 'divine' })]
				})
			})
		);
		expect(headline).toContain('from 5 divine');
	});

	/** A trailing `.0` on every whole price reads as precision the ask does not have. */
	it('prints a whole seller amount without a decimal tail', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ listings: [listing({ amount: 12 })] }) })
		);
		expect(headline).toContain('from 12 divine');
	});

	it('keeps one decimal on a fractional seller amount', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ listings: [listing({ amount: 2.5 })] }) })
		);
		expect(headline).toContain('from 2.5 divine');
	});

	/**
	 * Boundary: a count with nothing to quote. Still not "none found" — the
	 * market has matches, this fetch just brought back no row to price.
	 */
	it('reports the count alone when the query matched but no row came back', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ total: 6, listings: [] }) })
		);
		expect(headline).toBe('6 listings');
	});

	/**
	 * `truncated` means the 35-filter cap dropped tier loosening or whole
	 * support cells, so these listings answer a looser question than the
	 * capture describes. Without the caveat the user reads them as exact.
	 */
	it('warns that a capped query was loosened', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ truncated: true }) })
		);
		expect(headline).toBe(
			'4 listings · from 5 divine · looser query — fewer filters than the capture'
		);
	});

	it('leaves the loosening caveat off an untruncated query', () => {
		const headline = tradeHeadline(
			state({ status: 'done', result: result({ truncated: false }) })
		);
		expect(headline).not.toContain('looser query');
	});

	/** Defensive: Rust writes `done` and the result together, so a `done` with
	 *  nothing to show is a bug elsewhere — not a headline to invent. */
	it('prints nothing for a done state carrying no result', () => {
		expect(tradeHeadline(state({ status: 'done', result: null }))).toBeNull();
	});

	/**
	 * The badge already says `waiting for league` / `queued` / `searching`.
	 * Repeating it one line lower is noise, and it would push a retained
	 * result's line off the card while a re-search runs.
	 */
	it.each(['waiting-league', 'queued', 'searching'] as MercTradeStatus[])(
		'prints nothing while %s',
		(status) => {
			expect(tradeHeadline(state({ status }))).toBeNull();
		}
	);

	it('shows the error text Rust reported when the search failed', () => {
		expect(tradeHeadline(state({ status: 'error', error: 'trade API returned 429' }))).toBe(
			'trade API returned 429'
		);
	});

	it('falls back to a generic failure when an error carries no text', () => {
		expect(tradeHeadline(state({ status: 'error', error: null }))).toBe('search failed');
	});

	it('words a cancelled error as a cancellation rather than as an error text', () => {
		expect(tradeHeadline(state({ status: 'error', error: 'cancelled' }))).toBe('search cancelled');
	});

	/**
	 * The auto-search toggled off keeps the result it already fetched. Rust
	 * reaches `idle` there without clearing `result`, and the listings below
	 * are still on screen — a blank line above them would leave them unlabelled.
	 */
	it('keeps showing a retained result while idle', () => {
		const headline = tradeHeadline(
			state({ status: 'idle', queryHash: 'a1b2c3', result: result({ total: 4 }) })
		);
		expect(headline).toBe('4 listings · from 5 divine');
	});

	/**
	 * `compose_snapshot` forces only the STATUS to `off` when the module is
	 * off; `result` and `url` survive, like the capture and verdict cards. The
	 * headline has to survive with them or the listings lose their caption.
	 */
	it('keeps showing a retained result while the module is off', () => {
		const headline = tradeHeadline(
			state({ status: 'off', queryHash: 'a1b2c3', result: result({ total: 4 }) })
		);
		expect(headline).toBe('4 listings · from 5 divine');
	});

	it('prints nothing for an off state holding nothing to show', () => {
		expect(tradeHeadline(state({ status: 'off' }))).toBeNull();
	});

	it('prints nothing for a bare idle state', () => {
		expect(tradeHeadline(state({ status: 'idle' }))).toBeNull();
	});

	/**
	 * The budget-spent arm: Rust's `UrlOnly` drops the result and keeps the
	 * url, and without this line the card would go silent at exactly the moment
	 * the user needs telling that the link is now the only way on.
	 */
	it('says the search budget is spent when the ceiling is reached with a live link', () => {
		const headline = tradeHeadline(
			state({ status: 'idle', url: 'https://www.pathofexile.com/trade/search/Mirage/abc', searchesUsed: 3 })
		);
		expect(headline).toBe('search budget spent — link still live');
	});

	/** Boundary, one search below the ceiling: nothing to announce yet. */
	it('says nothing about the budget one search below the ceiling', () => {
		const headline = tradeHeadline(
			state({ status: 'idle', url: 'https://www.pathofexile.com/trade/search/Mirage/abc', searchesUsed: 2 })
		);
		expect(headline).toBeNull();
	});

	/** No link means there is nothing "still live" to point at. */
	it('says nothing about the budget when there is no link to fall back on', () => {
		expect(tradeHeadline(state({ status: 'idle', url: null, searchesUsed: 3 }))).toBeNull();
	});

	/**
	 * A retained result outranks the budget note: the listings ARE the answer
	 * the budget bought, and telling the user the budget is gone while the
	 * answer sits below it buries the answer.
	 */
	it('prefers a retained result over the budget note when both apply', () => {
		const headline = tradeHeadline(
			state({
				status: 'idle',
				url: 'https://www.pathofexile.com/trade/search/Mirage/abc',
				result: result({ total: 4 }),
				searchesUsed: 3
			})
		);
		expect(headline).toBe('4 listings · from 5 divine');
	});
});

/**
 * The number the wording quotes is Rust's, not this file's: `search.rs` decides
 * when the budget is spent (`TriggerAction::UrlOnly`) and this constant only
 * lets the page say the same figure. A drift here would have the page announce
 * a ceiling the trigger does not enforce.
 */
describe('MERC_TRADE_MAX_SEARCHES', () => {
	it('mirrors the per-session ceiling of three searches that Rust enforces', () => {
		expect(MERC_TRADE_MAX_SEARCHES).toBe(3);
	});
});
